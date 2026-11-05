"""Combine the two native runs and retain the evidence for the two new lever failures."""
import json
import argparse
from pathlib import Path
from types import SimpleNamespace

from runner import main_requests
from safety import Log, save

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("first", type=Path)
parser.add_argument("second", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
output = args.output
output.mkdir(parents=True, exist_ok=False)
runs = [json.loads((root / "results.json").read_text()) for root in [args.first, args.second]]
first = {row["check"]: row for row in runs[0]}
second = {row["check"]: row for row in runs[1]}
assert len(first) == 18 and len(second) == 13 and second.keys() <= first.keys()
classes = {"02-advisor": "b", "04-effort": "a", "05-claude-lockout": "b",
           "05-claude-sign-in": "b", "07-subagent": "a", "08-nonstream-310s": "b"}
latest = []
for name, initial in first.items():
    final = second.get(name, initial)
    latest.append(dict(check=name, run_1=initial["status"], run_2=second[name]["status"] if name in second else None,
                       status=final["status"], failure_class=classes.get(name),
                       source=f"native-run-{2 if name in second else 1}/{name}/result.json:9"))
save(output / "latest-results.json", latest)
save(output / "counts.json", dict(run_1={status.lower(): sum(r["status"] == status for r in runs[0]) for status in ["PASS", "FAIL"]},
                                  run_2={status.lower(): sum(r["status"] == status for r in runs[1]) for status in ["PASS", "FAIL"]},
                                  latest=dict(passed=sum(r["status"] == "PASS" for r in latest),
                                              failed=sum(r["status"] == "FAIL" for r in latest)),
                                  failure_classes={c: sum(r["failure_class"] == c for r in latest) for c in ["a", "b", "c"]}))
diagnosis = Log(output / "diagnosis.ndjson")
for name in ["04-effort", "07-subagent"]:
    root = args.second / name
    requests = json.loads((root / "request-index.json").read_text())
    recorder = SimpleNamespace(requests=requests)
    if name == "04-effort":
        diagnosis.event(check=name, main_requests=[dict(id=r["id"], effort=r["body"].get("output_config"),
                                                        status=r.get("status")) for r in main_requests(recorder)],
                        auxiliary_request_ids=[r["id"] for r in requests if r not in main_requests(recorder)],
                        terminal_evidence=["client.log:73 confirmation dialog", "client.log:83 selected Yes",
                                           "client.log:104 effort saved high; no second main request"],
                        failure_class="a", cause="driver sent the second prompt into the effort confirmation dialog")
    else:
        wire = [json.loads(line) for line in (root / "wire.ndjson").read_text().splitlines()]
        error = next(e for e in wire if e.get("event") == "error")
        parent = main_requests(recorder, "SC239-MAIN")
        child = main_requests(recorder, "SC239-SUBAGENT")
        diagnosis.event(check=name, parent_request_ids=[r["id"] for r in parent], child_request_ids=[r["id"] for r in child],
                        error_request_id=error["id"], error_at=error["at"],
                        parent_success_end=next(r["ended"] for r in parent if r["id"] == 4),
                        child_retry_status=child[-1]["status"], retry_after=child[-1]["response_headers"]["retry-after"],
                        run_seconds=json.loads((root / "result.json").read_text())["seconds"],
                        failure_class="a", cause="driver completed on the parent while the child was in its 30-second retry wait",
                        child_after_30_seconds="not observed in this prematurely stopped run")
