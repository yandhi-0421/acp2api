"""Bounds and redaction shared by the SC-003 research levers."""
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time

LOG_CAP = 4 * 1024 * 1024
REAL_HOME = str(Path.home())
REAL_USER = Path.home().name
CHILDREN = set()


def redact(text):
    text = text.replace(REAL_HOME, "~").replace(REAL_USER, "<user>")
    text = re.sub(r"[\w.+-]+(?:@|%40)[\w.-]+", "<email>", text, flags=re.I)
    text = re.sub(r"(?i)(client_id=)[^&\s\"\\]+", r"\1<redacted>", text)
    return text


class Log:
    def __init__(self, path, cap=LOG_CAP):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.touch(exist_ok=False)
        self.cap = cap
        self.lock = threading.Lock()

    def write(self, text):
        data = redact(text).encode()
        with self.lock:
            if self.path.stat().st_size + len(data) > self.cap:
                raise RuntimeError(f"log cap: {self.path.name}")
            with self.path.open("ab") as file:
                file.write(data)

    def event(self, **value):
        self.write(json.dumps(value, ensure_ascii=False) + "\n")


def save(path, value):
    Log(path).write(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def stop(process):
    # Also kill descendants when the parent has already exited.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=5)


def command(argv, output, seconds, env=None, cwd=None):
    log = Log(output)
    start = time.monotonic()
    process = subprocess.Popen(argv, env=env, cwd=cwd, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, start_new_session=True)
    CHILDREN.add(process)
    reason = []

    def drain():
        try:
            # Bound even a line with no newline. Decode each line before redaction.
            while data := process.stdout.readline(256 * 1024):
                if len(data) == 256 * 1024 and not data.endswith(b"\n"):
                    raise RuntimeError("command line cap")
                log.write(data.decode("utf-8", "replace"))
        except Exception as error:
            reason.append(str(error))
            stop(process)

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()
    try:
        process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        reason.append("wall timeout")
    finally:
        stop(process)
        CHILDREN.discard(process)
        reader.join(timeout=5)
    result = dict(exit_code=process.returncode, seconds=round(time.monotonic() - start, 3),
                  timeout_seconds=seconds, log_cap_bytes=LOG_CAP, stopped_by=reason)
    save(Path(output).with_suffix(".json"), result)
    return result


if __name__ == "__main__":
    import sys
    if sys.argv[1] == "--limit-exec":
        import resource
        resource.setrlimit(resource.RLIMIT_FSIZE, (LOG_CAP, LOG_CAP))
        os.execvpe(sys.argv[2], sys.argv[2:], os.environ)
    path, seconds, *argv = sys.argv[1:]
    result = command(argv, path, float(seconds))
    print(json.dumps(result))
    sys.exit(bool(result["exit_code"] or result["stopped_by"]))
