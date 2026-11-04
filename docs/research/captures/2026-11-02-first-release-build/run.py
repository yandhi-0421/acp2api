#!/usr/bin/env python3
"""Run one command with a wall timeout and a capped log; retain its result."""
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

ROOT = Path(os.environ["ACP56_PROBE_OUTPUT"]).resolve()
ROOT.mkdir(parents=True, exist_ok=True)
PROJECT = Path(__file__).resolve().parents[4]
(ROOT / "tmp").mkdir(exist_ok=True)
name, seconds, *command = sys.argv[1:]
env = os.environ.copy()
env.update(TMPDIR=str(ROOT / "tmp"), CARGO_PROFILE_DEV_DEBUG="0",
           CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="4",
           CARGO_TARGET_DIR=os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")),
           ACP2API_ACCEPTANCE_FEATURES=str(PROJECT / "features/acp2api"))
cap = 4 * 1024 * 1024
start = time.monotonic()
p = subprocess.Popen(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                     start_new_session=True)
sel = selectors.DefaultSelector()
sel.register(p.stdout, selectors.EVENT_READ)
reason = None
size = 0
with (ROOT / (name + ".log")).open("wb") as log:
    while sel.get_map():
        if time.monotonic() - start > float(seconds):
            reason = "timeout"
        if reason:
            os.killpg(p.pid, signal.SIGKILL)
            break
        for key, _ in sel.select(0.1):
            chunk = os.read(key.fileobj.fileno(), 65536)
            if not chunk:
                sel.unregister(key.fileobj)
                continue
            log.write(chunk[:cap - size])
            log.flush()
            size += min(len(chunk), cap - size)
            if size == cap:
                reason = "log cap"
    p.wait(timeout=10)
result = dict(command=command, exit_code=p.returncode, seconds=round(time.monotonic()-start, 3),
              timeout_seconds=float(seconds), log_cap_bytes=cap, log_bytes=size, stopped_by=reason)
(ROOT / (name + ".json")).write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
print((ROOT / (name + ".log")).read_text(errors="replace")[-5000:])
sys.exit(p.returncode if p.returncode else bool(reason))
