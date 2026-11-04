"""Measure one bounded first start of the isolated host with an empty data folder."""
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import statistics
import struct
import subprocess
import tempfile
import threading
import time

root=Path(__file__).resolve().parent
lib=ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
ready=threading.Event()
over_cap=threading.Event()
samples=[]
with tempfile.TemporaryDirectory(prefix='acp56-cold-') as temporary:
    folder=Path(temporary)
    with socket.socket() as lease:
        lease.bind(('127.0.0.1',0))
        port=lease.getsockname()[1]
    env=os.environ.copy()
    env['ACP56_ACCOUNT_ROOT']=str(folder/'account')
    p=subprocess.Popen([str(root/'measurement-host'),'--data-dir',str(folder/'data'),'--port',str(port)],
                       stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
    def read():
        count=0
        while line:=p.stdout.readline(64*1024+1):
            count+=len(line)
            if count>64*1024:
                over_cap.set()
                return
            if f'management panel at http://127.0.0.1:{port}/'.encode() in line:
                ready.set()
    threading.Thread(target=read,daemon=True).start()
    start=time.monotonic()
    ready_at=None
    try:
        while True:
            now=time.monotonic()-start
            assert now<30,'cold-start timeout: 30 s'
            assert not over_cap.is_set(),'terminal output cap: 64 KiB'
            assert p.poll() is None,'product stopped before sampling ended'
            assert all(f.stat().st_size<=64*1024 for f in (folder/'data/logs').glob('*') if f.is_file()),'log cap: 64 KiB'
            if ready.is_set() and ready_at is None:
                ready_at=now
            phase='before_address' if ready_at is None else 'ready' if now-ready_at<1 else 'idle'
            b=ctypes.create_string_buffer(1024)
            n=lib.proc_pidinfo(p.pid,4,0,b,1024)
            if n:
                assert n==96
                samples.append({'seconds':round(now,4),'phase':phase,'rss_bytes':struct.unpack_from('QQ',b.raw)[1]})
            if ready_at is not None and now-ready_at>=6:
                break
            time.sleep(.005)
    finally:
        if p.poll() is None:
            p.send_signal(signal.SIGINT)
            try:
                p.wait(timeout=10)
            except subprocess.TimeoutExpired:
                p.kill(); p.wait(timeout=5)
    assert p.returncode==0
summary={}
for phase in ['before_address','ready','idle']:
    group=[s for s in samples if s['phase']==phase]
    tail=[s['rss_bytes'] for s in group if s['seconds']>=group[-1]['seconds']-1]
    summary[phase]={'peak_bytes':max(s['rss_bytes'] for s in group),'steady_bytes':int(statistics.median(tail)),'samples':len(group)}
(root/'cold-memory.json').write_text(json.dumps({'summary':summary,'samples':samples,'requests':0,'interval_seconds':.005},indent=2)+'\n')
print(json.dumps(summary,indent=2))
