"""Summarize a retained native capture; inspect scratch metadata, never its contents."""
import argparse
import json
from pathlib import Path

from runner import acp_calls, disk_usage
from safety import Log, save

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("native", type=Path)
parser.add_argument("scratch", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
results = json.loads((args.native / "results.json").read_text())
save(args.output / "counts.json", {status: sum(r["status"] == status for r in results)
                                   for status in ["PASS", "FAIL", "BLOCKED"]})
disk = Log(args.output / "disk-diagnosis.ndjson")
for result in results:
    if result.get("error") != "scratch folder cap: 128 MiB":
        continue
    root = args.scratch / result["check"]
    assert root.is_dir(), f"retained scratch missing: {result['check']}"
    owned = disk_usage([root])
    apparent = 0
    targets = []
    for path in root.rglob("*"):
        try:
            if path.is_file():
                info = path.stat()
                apparent += info.st_size
                if path.is_symlink():
                    targets.append(dict(path=str(path.relative_to(root)), target_bytes=info.st_size,
                                        target_device=info.st_dev, target_inode=info.st_ino))
        except FileNotFoundError:
            continue
    disk.event(check=result["check"], source="retained scratch lstat/readlink/stat metadata only",
               owned_bytes=owned["owned_bytes"], old_dereferenced_bytes=apparent,
               symlinks=owned["symlinks"], targets=targets)


def differences(before, after, path=""):
    if type(before) is not type(after):
        return [dict(path=path, before=before, after=after)]
    if isinstance(before, dict):
        return [item for key in sorted(before.keys() | after.keys())
                for item in differences(before.get(key), after.get(key), path + "/" + key)]
    if isinstance(before, list):
        if len(before) != len(after):
            return [dict(path=path, before=before, after=after)]
        return [item for i, pair in enumerate(zip(before, after))
                for item in differences(*pair, path + "/" + str(i))]
    return [] if before == after else [dict(path=path, before=before, after=after)]

state = Log(args.output / "state-request-differences.ndjson")
for name in ["05-claude-lockout", "05-claude-sign-in"]:
    requests = json.loads((args.native / name / "request-index.json").read_text())
    assert len(requests) == 2
    state.event(check=name, acp_prompt_calls=sum(c["method"] == "session/prompt" for c in acp_calls((args.native / name / "host.log").read_text())), ids=[r["id"] for r in requests],
                gap_seconds=requests[1]["at"] - requests[0]["at"],
                body_differences=differences(requests[0]["body"], requests[1]["body"]),
                header_differences=differences(requests[0]["headers"], requests[1]["headers"]),
                status=[r["status"] for r in requests],
                should_retry=[r["response_headers"]["x-should-retry"] for r in requests],
                sdk_retry_count=[r["headers"]["x-stainless-retry-count"] for r in requests])
requests = json.loads((args.native / "08-nonstream-310s/request-index.json").read_text())
long = next(r for r in requests if "client_disconnected_at" in r)
later = [r for r in requests if r["at"] > long["at"]]
save(args.output / "nonstream-disconnect.json", dict(
    request_id=long["id"], started=long["at"], disconnected=long["client_disconnected_at"],
    duration_seconds=long["client_disconnected_at"] - long["at"],
    advertised_timeout=long["headers"].get("x-stainless-timeout"),
    status=long.get("status"), retry_ids=[r["id"] for r in later],
    retry_gap_seconds=later[0]["at"] - long["client_disconnected_at"],
    retry_body_differences=differences(long["body"], later[0]["body"])))
