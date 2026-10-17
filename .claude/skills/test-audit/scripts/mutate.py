#!/usr/bin/env python3
"""Run hand-written mutants in a git worktree, one at a time, and restore each file.

Usage:
  mutate.py --copy <worktree> --mutants <file.json> --cmd '<test command>'
            [--timeout <seconds>] [--out <dir>]

The mutant file is a JSON array. Each object has "id", "path", "line", "old", and
"new", and an optional "cmd" that replaces --cmd for that mutant. "line" starts at 1.
"old" must occur exactly once on that line.

Results: killed (the command fails), survived (the command passes), timeout, error.
Exit codes: 0 done, 2 bad input or a refused copy, 3 a run without a mutant failed,
4 the copy changed, or a process of an earlier run still acts on the copy after a timeout,
5 a signal stopped the run (make a new copy before the next run).

Each write gives the file a new modification time, later than the time before the write
and than each earlier write. Thus a cache that uses the time and the size, such as
Python bytecode, sees the change. --timeout also limits the runs without a mutant.
"""

import argparse
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True


class Stop(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def on_signal(number, frame):
    raise Stop(5, f"Signal {number} stopped the run. Make a new copy before the next run.")


LAST_STAMP = [0]


def write(target, data):
    before = int(target.stat().st_mtime)
    target.write_bytes(data)
    stamp = max(int(time.time()) + 1, LAST_STAMP[0] + 1, before + 1)
    os.utime(target, (stamp, stamp))
    LAST_STAMP[0] = stamp


def git(copy, *args):
    done = subprocess.run(["git", "-C", str(copy), *args], capture_output=True, text=True, errors="surrogateescape")
    if done.returncode != 0:
        raise Stop(2, f"git {' '.join(args)} failed in {copy}: {done.stderr.strip()}")
    return done.stdout


def check_copy(copy):
    if not copy.is_dir():
        raise Stop(2, f"{copy} is not a directory.")
    top = Path(git(copy, "rev-parse", "--show-toplevel").strip()).resolve()
    if top != copy:
        raise Stop(2, f"{copy} is not the top directory of a worktree.")
    dirs = git(copy, "rev-parse", "--git-dir", "--git-common-dir").split("\n")
    git_dir, common_dir = ((copy / d).resolve() for d in dirs[:2])
    if git_dir == common_dir:
        raise Stop(2, f"{copy} is a main working tree. Use a linked worktree: git worktree add --detach <dir>.")
    if subprocess.run(["git", "-C", str(copy), "symbolic-ref", "-q", "HEAD"], capture_output=True).returncode == 0:
        raise Stop(2, f"{copy} has a branch. Use a copy with a detached HEAD: git worktree add --detach <dir>.")


def load_mutants(path, copy):
    try:
        mutants = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise Stop(2, f"Cannot read the mutant file: {error}")
    if not isinstance(mutants, list) or not mutants:
        raise Stop(2, "The mutant file must hold a JSON array with one or more objects.")
    seen = set()
    for m in mutants:
        ok = (
            isinstance(m, dict)
            and isinstance(m.get("id"), str) and m["id"] and m["id"] not in seen
            and "/" not in m["id"] and not m["id"].lower().startswith("no-mutant-") and isinstance(m.get("path"), str)
            and isinstance(m.get("line"), int) and not isinstance(m["line"], bool) and m["line"] >= 1
            and isinstance(m.get("old"), str) and m["old"] and "\n" not in m["old"]
            and isinstance(m.get("new"), str) and "\n" not in m["new"] and m["new"] != m["old"]
            and isinstance(m.get("cmd", ""), str)
        )
        if not ok:
            raise Stop(2, f"Bad mutant: {json.dumps(m)}. Give a new id, a path, a line, one-line old and new texts that differ, and an optional cmd.")
        seen.add(m["id"])
        target = (copy / m["path"]).resolve()
        if copy not in target.parents or not target.is_file():
            raise Stop(2, f"Mutant {m['id']}: {m['path']} is not a file in the copy.")
    return mutants


def run(cmd, copy, log, timeout):
    """Run cmd in copy. Return (exit code or None for a timeout, seconds)."""
    start = time.monotonic()
    with open(log, "wb") as out:
        proc = subprocess.Popen(cmd, shell=True, cwd=copy, stdout=out, stderr=subprocess.STDOUT,
                                stdin=subprocess.DEVNULL, start_new_session=True)
        code = None
        try:
            code = proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            pass
        finally:
            # Stop each process of the run, also a process that the tests left in the background.
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            proc.wait()
    return code, time.monotonic() - start


def apply(m, copy):
    """Change the file of mutant m. Return the bytes before the change, or None if old is not on the line once."""
    target = copy / m["path"]
    original = target.read_bytes()
    lines = original.decode("utf-8", "surrogateescape").split("\n")
    if m["line"] > len(lines) or lines[m["line"] - 1].count(m["old"]) != 1:
        return None
    lines[m["line"] - 1] = lines[m["line"] - 1].replace(m["old"], m["new"])
    write(target, "\n".join(lines).encode("utf-8", "surrogateescape"))
    return original


def main():
    parser = argparse.ArgumentParser(description="Run mutants in a git worktree.")
    parser.add_argument("--copy", required=True, help="a linked git worktree at the head of the scope")
    parser.add_argument("--mutants", required=True, help="the JSON file of the mutants")
    parser.add_argument("--cmd", required=True, help="the test command, run with /bin/sh in the copy")
    parser.add_argument("--timeout", type=float, help="the time limit of one run, in seconds")
    parser.add_argument("--out", help="the directory of the logs (default: <copy>-logs)")
    args = parser.parse_args()
    for number in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        if signal.getsignal(number) is not signal.SIG_IGN:
            signal.signal(number, on_signal)

    copy = Path(args.copy).resolve()
    check_copy(copy)
    mutants = load_mutants(args.mutants, copy)
    out = Path(args.out).resolve() if args.out else Path(f"{copy}-logs")
    if out == copy or copy in out.parents:
        raise Stop(2, "The log directory must be outside the copy.")
    out.mkdir(parents=True, exist_ok=True)

    limits = {}
    for n, cmd in enumerate(dict.fromkeys([args.cmd] + [m["cmd"] for m in mutants if m.get("cmd")])):
        code, seconds = run(cmd, copy, out / f"no-mutant-{n}.log", args.timeout)
        if code != 0:
            how = "timed out" if code is None else f"exit {code}"
            raise Stop(3, f"The run without a mutant failed ({how}): {cmd}. See {out}/no-mutant-{n}.log.")
        limits[cmd] = args.timeout or max(60.0, 3 * seconds)
    status = git(copy, "status", "--porcelain"), git(copy, "diff", "HEAD", "--binary")

    rows = []
    try:
        for m in mutants:
            cmd = m.get("cmd") or args.cmd
            original = apply(m, copy)
            if original is None:
                rows.append((m, "error", "-", 0.0))
                continue
            try:
                code, seconds = run(cmd, copy, out / f"{m['id']}.log", limits[cmd])
            finally:
                write(copy / m["path"], original)
            result = "timeout" if code is None else ("survived" if code == 0 else "killed")
            rows.append((m, result, "-" if code is None else str(code), seconds))
            current = git(copy, "status", "--porcelain"), git(copy, "diff", "HEAD", "--binary")
            if (copy / m["path"]).read_bytes() != original or current != status:
                raise Stop(4, f"The copy changed after mutant {m['id']}. Look at git status in {copy}.")
            if code is None and run(cmd, copy, out / f"no-mutant-after-{m['id']}.log", limits[cmd])[0] != 0:
                raise Stop(4, f"After the timeout of mutant {m['id']}, the run without a mutant failed. "
                              f"A process of the run can still act on the copy. See {out}/no-mutant-after-{m['id']}.log.")
    finally:
        print("id\tlocation\tresult\texit\tseconds")
        for m, result, code, seconds in rows:
            print(f"{m['id']}\t{m['path']}:{m['line']}\t{result}\t{code}\t{seconds:.1f}")
        counts = {r: sum(1 for row in rows if row[1] == r) for r in ("killed", "survived", "timeout", "error")}
        print(" ".join(f"{r}={n}" for r, n in counts.items()) + f" logs={out}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Stop as stop:
        print(f"ERROR: {stop}", file=sys.stderr)
        sys.exit(stop.code)
