"""Measure only the product host PID; exclude the probe and fake ACP processes."""
import ctypes
import json
import os
from pathlib import Path
import queue
import signal
import statistics
import struct
import subprocess
import sys
import threading
import time

root=Path(__file__).resolve().parent
lib=ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
p=subprocess.Popen([str(root/'probe'),str(root/'measurement-host')],stdout=subprocess.PIPE,
                   stderr=subprocess.PIPE,text=True,start_new_session=True)
events=queue.Queue()
def read():
    size=0
    for line in p.stdout:
        size+=len(line)
        assert size<=64*1024,'probe output cap'
        events.put(json.loads(line))
threading.Thread(target=read,daemon=True).start()
phase=None
pid=None
probe_root=None
samples=[]
proof=[]
started=time.monotonic()
success=False
try:
    while p.poll() is None or not events.empty():
        assert time.monotonic()-started<80,'measurement timeout'
        while not events.empty():
            event=events.get()
            if 'probe_root' in event:
                probe_root=Path(event.pop('probe_root'))
            proof.append(event)
            if 'phase' in event:
                phase=event['phase']
                pid=event['pid']
        if pid:
            if probe_root:
                assert all(f.stat().st_size<=64*1024 for f in (probe_root/'data/logs').glob('*') if f.is_file()), 'product log cap: 64 KiB'
            b=ctypes.create_string_buffer(1024)
            n=lib.proc_pidinfo(pid,4,0,b,1024)
            if n:
                assert n==96,('unexpected proc_taskinfo length',n)
                resident=struct.unpack_from('QQ',b.raw)[1]
                samples.append({'seconds':round(time.monotonic()-started,3),'phase':phase,'rss_bytes':resident})
        time.sleep(.05)
    assert p.returncode==0,('probe failed',p.stderr.read(4096))
    assert len([e for e in proof if e.get('status')==200])==3
    assert proof[-1]['runtime_processes']==2
    assert proof[-1]['fake_failure'] is None
    success=True
finally:
    if not success:
        try:
            os.killpg(p.pid,signal.SIGINT)
            deadline=time.monotonic()+7
            while time.monotonic()<deadline:
                p.poll()
                os.killpg(p.pid,0)
                time.sleep(.05)
            os.killpg(p.pid,signal.SIGKILL)
        except ProcessLookupError:
            pass
        p.wait(timeout=5)
summary={}
for name in dict.fromkeys(s['phase'] for s in samples):
    group=[s for s in samples if s['phase']==name]
    last=group[-1]['seconds']
    tail=[s['rss_bytes'] for s in group if s['seconds']>=last-1]
    summary[name]={'samples':len(group),'peak_bytes':max(s['rss_bytes'] for s in group),
                   'steady_bytes':int(statistics.median(tail))}
result={'method':'proc_pidinfo(PROC_PIDTASKINFO=4), resident_size, 50 ms samples',
        'scope':'release library through production ports; isolated account-root measurement host',
        'summary':summary,'proof':proof,'samples':samples}
assert (len(json.dumps(result)))<1024*1024
(root/(sys.argv[1] if len(sys.argv)>1 else 'memory.json')).write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'summary':summary,'proof':proof},indent=2))
