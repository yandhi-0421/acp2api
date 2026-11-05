"""Exercise the levers' refusal, redaction, timeout, log cap, and pty boundaries."""
import http.client
from pathlib import Path
import sys
import tempfile
import time
from types import SimpleNamespace

from runner import Check, Process, Recorder, evaluate, wait_for
from safety import Log, REAL_HOME, REAL_USER, command, redact, save

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=False)
results = []
sample = f"{REAL_HOME}/test {REAL_USER} probe@example.invalid probe%40example.invalid client_id=fake-id&ok=1"
clean = redact(sample)
assert all(secret not in clean for secret in [REAL_HOME, REAL_USER, "@", "%40", "fake-id"])
results.append("home, user, plain email, encoded email, OAuth client_id redaction")

log = Log(output / "small.log", cap=8)
log.write("12345678")
try:
    log.write("9")
except RuntimeError:
    assert log.path.stat().st_size == 8
else:
    raise AssertionError("log cap did not refuse the next byte")
results.append("log cap refuses overflow without a partial write")

timed = command([sys.executable, "-c", "import time; time.sleep(5)"], output / "timeout.log", 0.1)
assert timed["exit_code"] != 0 and timed["stopped_by"] == ["wall timeout"]
results.append("wall timeout kills and reaps the command")

recorder = Recorder(output, 9, 5, cap=1)
try:
    for expected in [200, 400]:
        conn = http.client.HTTPConnection("127.0.0.1", recorder.port, timeout=3)
        conn.request("GET", "/__guard")
        reply = conn.getresponse()
        assert reply.status == expected
        reply.read()
        conn.close()
    conn = http.client.HTTPConnection("127.0.0.1", recorder.proxy.server_port, timeout=3)
    conn.request("CONNECT", "api.anthropic.com:443")
    reply = conn.getresponse()
    assert reply.status == 403
    reply.read()
    conn.close()
    assert recorder.count == 2 and recorder.requests == [] and recorder.proxy_count == 1
    assert recorder.failures == ["request cap"]
finally:
    recorder.close()
results.append("HTTP cap refuses request 2 before any upstream call")
results.append("CONNECT receives 403 from the local refusal proxy")

code = '''import os, tty
assert os.isatty(0)
tty.setraw(0)
os.write(1,b"\\x1b[6n")
assert os.read(0,6) == b"\\x1b[1;1R"
os.write(1,b"SC239-PTY-OK\\n")
'''
process = Process([sys.executable, "-c", code], output / "pty.log", {}, output, terminal=True)
try:
    process.child.wait(timeout=5)
    process.thread.join(timeout=2)
    assert process.child.returncode == 0 and "SC239-PTY-OK" in process.text()
finally:
    process.close()
results.append("pty driver answers the terminal cursor query and drains output")
code = 'from pathlib import Path; Path("child.log").write_bytes(b"x"*(4*1024*1024+1))'
process = Process([sys.executable, "-c", code], output / "file-limit.log", {}, output)
try:
    process.child.wait(timeout=5)
    file_limit_bytes = (output / "child.log").stat().st_size
    assert process.child.returncode != 0 and file_limit_bytes <= 4 * 1024 * 1024
finally:
    process.close()
results.append("RLIMIT_FSIZE stops a child-owned log at 4 MiB")
(output / "child.log").unlink()
# Installed client helpers are symlinks, not copies owned by this run.
with tempfile.TemporaryDirectory(dir=output) as folder:
    root = Path(folder)
    scratch = root / "scratch"
    scratch.mkdir()
    target = root / "installed-executable"
    with target.open("wb") as file:
        file.truncate(129 * 1024 * 1024)
    (scratch / "helper").symlink_to(target)
    polls = iter([False, True])
    wait_for(lambda: next(polls), time.monotonic() + 1, [], work=scratch)
    (scratch / "helper").unlink()
    target.rename(scratch / "actual-copy")
    try:
        wait_for(lambda: False, time.monotonic() + 1, [], work=scratch)
    except RuntimeError as error:
        assert str(error) == "scratch folder cap: 128 MiB"
    else:
        raise AssertionError("actual scratch bytes escaped the 128 MiB cap")
results.append("scratch cap excludes symlink targets and still refuses an actual oversized copy")
body = dict(tools=[dict(name="probe")], messages=[dict(role="user", content="SC239-MAIN follow-up")])
replay = SimpleNamespace(requests=[dict(id=i, at=at, status=status, body=body)
                                  for i, at, status in [(1, 0, 200), (2, 1, 503), (3, 31, 200)]],
                         events=[dict(id=1, at=0.2, event="content_block_delta", frame="SC239-PARTIAL"),
                                 dict(id=1, at=0.3, event="content_block_stop"),
                                 dict(id=1, at=0.5, event="error")])
evaluate(Check("interactive-control", "claude", "after", "tui"), replay, "", "SC239-OK", None,
         ["follow-up in retry wait"])
try:
    evaluate(Check("print-control", "claude", "after"), replay, "", "SC239-OK", 0, [])
except AssertionError as error:
    assert str(error) == "after-text retry omitted the resume message"
else:
    raise AssertionError("print mode passed without its required resume message")
results.append("interactive follow-up needs no automatic resume text; print mode still requires it")
save(output / "result.json", dict(passed=len(results), failed=0, file_limit_bytes=file_limit_bytes, checks=results))
print(f"lever controls: {len(results)} passed, 0 failed")
