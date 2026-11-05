"""Re-evaluate retained native observations under the SC-003 decision of 2026-11-05."""
import argparse
import json
from pathlib import Path

from runner import CHECKS, check_state, nonstream_record

HERE = Path(__file__).resolve().parent
CONTRACT_HEAD = "f4df342d5a54022df1aeebc09c98f3ceb70ee295"


def reviewed_results():
    catalog = {check.name: check for check in CHECKS}
    outputs = {}
    runs = []
    latest = {}
    for number in [1, 2, 3]:
        root = HERE / f"native-run-{number}"
        evidence = {row["case"]: row for row in map(json.loads, (root / "evidence.ndjson").read_text().splitlines()[1:])}
        reviewed = []
        for original in json.loads((root / "results.json").read_text()):
            name = original["check"]
            if name not in catalog:
                continue
            check = catalog[name]
            result = json.loads((root / name / "result.json").read_text())
            result["recorded_status"] = result.get("recorded_status", result["status"])
            result["recorded_error"] = result.get("recorded_error", result.get("error"))
            result["status"] = result["recorded_status"]
            result["evaluation_head"] = CONTRACT_HEAD
            result["evaluation_kind"] = "record" if check.scenario == "nonstream" else "behavior"
            result["evaluation_reason"] = "The behavior rule is unchanged; the original result applies."
            if check.scenario in ["lockout", "sign-in", "nonstream"]:
                case = evidence[name]
                requests = [dict(request) for request in case["requests"] if request["body"].get("tools")
                            and set(request["direct_user_markers"]) & {"SC239-MAIN", "SC239-SUBAGENT"}]
                try:
                    if check.scenario == "nonstream":
                        details = nonstream_record(requests)
                    else:
                        for request in requests:
                            data = next(event["data"] for event in case["wire"]
                                        if event["id"] == request["id"] and "error" in event.get("data", {}))
                            request["response"] = json.dumps(data)
                        details = check_state(check, requests, case["acp_prompt_count"],
                                              "\n".join(error["text"] for error in case["client_errors"]), case["client_exit"])
                except AssertionError as error:
                    result["status"] = "FAIL"
                    result["evaluation_reason"] = str(error)
                else:
                    result["status"] = "PASS"
                    result["details"] = details
                    result["evaluation_reason"] = "The retained observation satisfies the revised SC-003 rule."
            if result["status"] == "PASS":
                for key in ["error", "error_type", "failure_line"]:
                    result.pop(key, None)
            outputs[root / name / "result.json"] = result
            reviewed.append(result)
            latest[name] = dict(check=name, status=result["status"], evaluation_kind=result["evaluation_kind"],
                                source=f"native-run-{number}/{name}/result.json:9", source_run=number)
        outputs[root / "results.json"] = reviewed
        runs.append(reviewed)
    rows = []
    for check in CHECKS:
        row = latest[check.name]
        row["runs"] = [next((result["status"] for result in run if result["check"] == check.name), None) for run in runs]
        rows.append(row)
    counts = dict(evaluation_head=CONTRACT_HEAD,
                  runs=[dict(run=n, total=len(run), passed=sum(r["status"] == "PASS" for r in run),
                             failed=sum(r["status"] == "FAIL" for r in run)) for n, run in enumerate(runs, 1)],
                  latest=dict(total=len(rows), passed=sum(r["status"] == "PASS" for r in rows),
                              failed=sum(r["status"] == "FAIL" for r in rows),
                              behavior_checks=sum(r["evaluation_kind"] == "behavior" for r in rows),
                              records=sum(r["evaluation_kind"] == "record" for r in rows)))
    outputs[HERE / "latest-results.json"] = rows
    outputs[HERE / "counts.json"] = counts
    return outputs, counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="update the reviewed results; default checks their bytes")
    args = parser.parse_args()
    outputs, counts = reviewed_results()
    for path, value in outputs.items():
        text = json.dumps(value, indent=2, ensure_ascii=False) + "\n"
        if args.write:
            path.write_text(text)
        else:
            assert path.read_text() == text, f"reviewed result differs: {path.relative_to(HERE)}"
    print(json.dumps(counts, indent=2))


if __name__ == "__main__":
    main()
