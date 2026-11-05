"""Exercise the SC-003 decisions of 2026-11-05 with synthetic API records."""
import argparse
import copy
import json
from pathlib import Path
from types import SimpleNamespace

from runner import CHECKS, Check, evaluate
from safety import save


def state_records():
    body = dict(model="gemini-3.8-flash", tools=[dict(name="SC239-probe")],
                messages=[dict(role="user", content="SC239-MAIN")],
                thinking=dict(type="adaptive", display="updates"))
    headers = {"anthropic-beta": "effort-2025-11-24,thinking-display-updates-2026-08-18",
               "x-stainless-retry-count": "0"}
    response = json.dumps(dict(type="error", error=dict(type="invalid_request_error",
                                                       message="acp2api: lockout since SC239")))
    first = dict(id=1, at=0.5, body=body, headers=headers, status=400,
                 response_headers={"x-should-retry": "false"}, response=response, ended=0.6)
    second = copy.deepcopy(first)
    second.update(id=2, at=0.7, ended=0.8)
    del second["body"]["thinking"]["display"]
    second["headers"]["anthropic-beta"] = "effort-2025-11-24"
    return [first, second]


def rejected(check, requests, host="", text="lockout since SC239", exit_code=1):
    try:
        evaluate(check, SimpleNamespace(requests=requests, events=[]), host, text, exit_code, [])
    except AssertionError:
        return
    raise AssertionError("an invalid state observation passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--only", choices=["state", "record", "catalog"])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    passed = []
    if args.only in [None, "state"]:
        check = Check("05-claude-lockout", "claude", "lockout")
        requests = state_records()
        details = evaluate(check, SimpleNamespace(requests=requests, events=[]), "", "lockout since SC239", 1, [])
        assert details["main_request_count"] == 2 and details["acp_prompt_calls"] == 0
        passed.append("one capability fallback receives two refusals without an ACP prompt")
        for client in ["claude", "codex"]:
            details = evaluate(Check("state-control", client, "lockout"),
                               SimpleNamespace(requests=requests[:1], events=[]), "", "lockout since SC239", 1, [])
            assert details["main_request_count"] == 1
            passed.append(client + " can stop after one state refusal")
        signed_out = copy.deepcopy(requests)
        for request in signed_out:
            request["response"] = json.dumps(dict(type="error", error=dict(type="invalid_request_error",
                message="acp2api: the official runtime is not signed in")))
        evaluate(Check("05-claude-sign-in", "claude", "sign-in"), SimpleNamespace(requests=signed_out, events=[]),
                 "", "the official runtime is not signed in", 1, [])
        passed.append("sign-in refusal permits the same capability fallback")
        for change in ["third request", "same capability", "changed prompt", "changed header", "second success", "other state"]:
            invalid = copy.deepcopy(requests)
            if change == "third request": invalid.append(copy.deepcopy(invalid[-1]))
            if change == "same capability": invalid[-1]["body"] = copy.deepcopy(invalid[0]["body"])
            if change == "changed prompt": invalid[-1]["body"]["messages"][0]["content"] = "SC239-MAIN changed"
            if change == "changed header": invalid[-1]["headers"]["anthropic-beta"] += ",other-capability"
            if change == "second success": invalid[-1]["status"] = 200
            if change == "other state": invalid[-1]["response"] = json.dumps(dict(error=dict(type="invalid_request_error", message="another refusal")))
            rejected(check, invalid)
            passed.append("state rule rejects " + change)
        rejected(Check("state-control", "codex", "lockout"), requests)
        passed.append("Codex still permits no second main request")
        host = 'SC239 ' + json.dumps(dict(kind="hub", event=dict(kind="got", line=json.dumps(dict(method="session/prompt")))))
        rejected(check, requests, host=host)
        passed.append("a state refusal cannot start an ACP prompt")
        rejected(check, requests, exit_code=None)
        passed.append("the API client must stop after the state refusal")
    if args.only in [None, "record"]:
        body = dict(stream=False, tools=[dict(name="SC239-probe")], messages=[dict(role="user", content="SC239-MAIN")])
        original = dict(id=3, at=31.0, body=body, headers={"x-stainless-timeout": "300"}, client_disconnected_at=331.037)
        replacement = dict(id=4, at=331.04, body=body, status=200, ended=331.1)
        for status in [200, None]:
            later = dict(replacement, status=status)
            details = evaluate(Check("08-nonstream-310s", "claude", "nonstream"),
                               SimpleNamespace(requests=[original, later], events=[]), "", "", None, [])
            assert details["original_request_id"] == 3 and round(details["duration_seconds"], 3) == 300.037
            assert details["replacement_requests"][0]["id"] == 4 and details["replacement_requests"][0]["status"] == status
            assert details["observation"] == "record"
            passed.append("the record preserves a replacement with status " + str(status))
        complete = dict(id=3, at=31.0, body=body, headers={}, status=200, ended=341.0)
        details = evaluate(Check("08-nonstream-310s", "claude", "nonstream"),
                           SimpleNamespace(requests=[complete], events=[]), "", "", 0, [])
        assert details["duration_seconds"] == 310 and details["replacement_requests"] == []
        passed.append("the record also accepts a completed original request")
    if args.only in [None, "catalog"]:
        assert len(CHECKS) == 17 and all(not check.name.startswith("02-") for check in CHECKS)
        passed.append("the native catalog has 17 variants without the retired item")
    save(args.output / "result.json", dict(passed=len(passed), failed=0, checks=passed))
    print(f"SC-003 controls: {len(passed)} passed, 0 failed")


if __name__ == "__main__":
    main()
