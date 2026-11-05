"""Reproduce the parent/child race with synthetic records and drive an effort terminal."""
import argparse
import copy
from pathlib import Path
import sys
import time
from types import SimpleNamespace

from runner import Check, Process, change_effort, evaluate, main_requests, reply_completed, wait_for
from safety import save

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
parser.add_argument("--only", choices=["subagent", "effort", "codex"])
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
passed = []
if args.only in [None, "subagent"]:
    parent = dict(tools=[{"name": "SC239-probe"}], messages=[
        {"role": "user", "content": "SC239-MAIN"},
        {"role": "assistant", "content": [{"type": "tool_use", "input": {"prompt": "SC239-SUBAGENT"}}]},
        {"role": "user", "content": [{"type": "tool_result", "content": "SC239-SUBAGENT"}]},
    ])
    child = dict(tools=[{"name": "SC239-probe"}], messages=[{"role": "user", "content": "SC239-SUBAGENT"}])
    resumed = copy.deepcopy(child)
    # This short match fragment exercises the same resume check as the native evaluator.
    resumed["messages"].append({"role": "user", "content": "Your response above was cut off mid-stream"})
    requests = [
        dict(id=1, at=0.5, body={"tools": [], "messages": []}, status=400),
        dict(id=2, at=1.0, body=parent, status=200, ended=1.5, response="SC239-Agent"),
        dict(id=3, at=1.6, body=child, status=200, ended=2.0, response="SC239-PARTIAL"),
        dict(id=4, at=2.1, body=parent, status=200, ended=2.2, response="SC239-OK"),
        dict(id=5, at=2.3, body=resumed, status=503, response_headers={"retry-after": "30"}),
    ]
    replay = SimpleNamespace(requests=requests, events=[
        dict(id=3, at=1.7, event="content_block_delta", frame="SC239-PARTIAL"),
        dict(id=3, at=1.9, event="content_block_stop"),
        dict(id=3, at=2.0, event="error"),
    ])
    assert not reply_completed(replay, "SC239-SUBAGENT"), "parent reply ended the child check before its retry"
    child_retry = copy.deepcopy(requests[-1])
    child_retry.update(id=6, at=32.1, status=200, ended=32.3, response="SC239-OK")
    requests.append(child_retry)
    assert reply_completed(replay, "SC239-SUBAGENT"), "successful child retry did not complete the check"
    details = evaluate(Check("07-subagent", "claude", "subagent", "tui"), replay, "", "SC239-OK", None, [])
    assert details["retry_gap_seconds"] > 30
    passed.append("parent completion cannot end the child check; a resumed child can")
if args.only in [None, "codex"]:
    codex = SimpleNamespace(requests=[dict(id=1, body=dict(
        tools=[{"name": "SC239-probe"}],
        input=[{"role": "user", "content": [{"type": "input_text", "text": "SC239-MAIN"}]}],
    ))])
    assert [r["id"] for r in main_requests(codex, "SC239-MAIN")] == [1], "Responses input_text lost its main-request marker"
    passed.append("Responses input_text retains the measured Codex main request")
if args.only in [None, "effort"]:
    # The terminal reproduces the confirmation and saved-high states from native run 2.
    code = '''import os, tty
tty.setraw(0)
def emit(text): os.write(1, (text+"\\n").encode())
state="ready"
emit("SC239-TERMINAL-READY")
while True:
    text=b""
    while not text.endswith(b"\\r"): text+=os.read(0,1)
    text=text[:-1].decode()
    if text=="/effort high":
        state="confirm"
        emit("Change effort level? 1. Yes, switch to high")
    elif state=="confirm":
        if text: emit("SC239-DISCARDED: "+text)
        state="high"
        emit("Set effort level to high")
    elif state=="high" and text.startswith("SC239-MAIN"):
        emit("SC239-SUBMITTED")
        break
'''
    process = Process([sys.executable, "-c", code], args.output / "effort-terminal.log", {}, args.output, terminal=True)
    try:
        deadline = time.monotonic() + 5
        wait_for(lambda: "SC239-TERMINAL-READY" in process.text(), deadline, [process])
        change_effort(process, deadline, [process])
        process.send("SC239-MAIN second effort turn. Reply with SC239-OK only.")
        wait_for(lambda: "SC239-SUBMITTED" in process.text() or "SC239-DISCARDED" in process.text(), deadline, [])
        assert "SC239-SUBMITTED" in process.text() and "SC239-DISCARDED" not in process.text(), "second prompt was consumed by the effort confirmation"
        process.child.wait(timeout=1)
        process.thread.join(timeout=1)
    finally:
        process.close()
    passed.append("effort confirmation completes before the next user prompt")
save(args.output / "result.json", dict(passed=len(passed), failed=0, checks=passed))
print(f"round-3 controls: {len(passed)} passed, 0 failed")
