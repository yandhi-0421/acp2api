"""Make a small redacted capture of one run.

  python3 extract.py <run-dir> <capture-dir>

The capture keeps hashes of all items. It keeps the text only of synthetic case
items. System prompts, tool schemas, and context items stay as hashes, because
they hold paths and text of the environment.
"""
import json
import os
import pathlib
import re
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import port

MARK = re.compile(r"(?<![A-Za-z0-9])[ABCDEPRTZ]\d\d(?![0-9])")
HOME = os.path.expanduser("~")
USER = os.path.basename(HOME)


def redactor(run):
    real = str(pathlib.Path(run).resolve())

    def redact(text):
        text = text.replace(f"{real}/work", "<work>").replace(real, "<run>")
        text = text.replace(HOME, "~").replace(USER, "<user>")
        return re.sub(r"[\w.+-]+(@|%40)[\w-]+\.[\w.-]+", "<email>", text)

    return redact


def part(tag, body, redact):
    entry = {
        "tag": tag,
        "len": len(body),
        "sha": port.sha(redact(body)),
        "normSha": port.sha(port.norm(redact(body))),
        "cleanSha": port.sha(port.clean(redact(body))),
    }
    if len(body) <= 32 and not body.strip():
        entry["text"] = body
    elif "completed with no output" in body or (MARK.search(body) and not body.startswith("<system-reminder>")):
        entry["text"] = redact(body)
    return entry


def item(entry, redact):
    out = {key: entry[key] for key in ("kind", "role", "id", "name", "error", "wire") if key in entry}
    if "arguments" in entry:
        out["arguments"] = redact(entry["arguments"])
        out["sha"] = port.sha(out["arguments"])
        out["normSha"] = port.sha(port.norm(out["arguments"]))
    if "parts" in entry:
        out["parts"] = [part(tag, body, redact) for tag, body in entry["parts"]]
    return out


def reference(entry, table, index):
    """Each request repeats the history, so keep each distinct item once."""
    key = json.dumps(entry, sort_keys=True, ensure_ascii=False)
    if key not in index:
        index[key] = len(table)
        table.append(entry)
    return index[key]


def main(run, capture):
    run = pathlib.Path(run).resolve()
    redact = redactor(run)
    summary = json.loads((run / "summary.json").read_text())
    requests = []
    table = []
    index = {}
    versions = set()
    other = {"side": 0, "get": 0}
    for line in (run / "requests.jsonl").open(encoding="utf-8", errors="surrogatepass"):
        record = json.loads(line)
        if "body" not in record:
            other["get"] += 1
            continue
        if not record.get("main"):
            other["side"] += 1
            continue
        agent = record.get("headers", {}).get("user-agent", "")
        versions.add(re.sub(r"\s*\(.*$", "", agent))
        body = record["body"]
        if record["path"].endswith("/chat/completions"):
            system, items = port.chat_items(body)
        elif record["path"].endswith("/responses"):
            system, items = port.responses_items(body)
        else:
            system, items = port.messages_items(body)
        tools = sorted(json.dumps(tool, sort_keys=True, ensure_ascii=False) for tool in body.get("tools") or [])
        requests.append(
            {
                "seq": record["seq"],
                "stepIndex": record["step_index"],
                "step": record["step"],
                "systemSha": port.sha(redact(system)),
                "systemNormSha": port.sha(port.norm(redact(system))),
                "systemLen": len(system),
                "toolCount": len(body.get("tools") or []),
                "toolSetSha": port.sha(redact("\n".join(tools))),
                "items": [reference(item(entry, redact), table, index) for entry in items],
            }
        )
    emitted = []
    for line in (run / "emitted.jsonl").open(encoding="utf-8", errors="surrogatepass"):
        record = json.loads(line)
        for field in ("text", "raw"):
            if field in record:
                record[field] = redact(record[field])
        emitted.append(record)
    refused = {}
    if (run / "proxy.log").exists():
        for line in (run / "proxy.log").read_text().splitlines():
            refused[line] = refused.get(line, 0) + 1
    capture = pathlib.Path(capture)
    capture.mkdir(parents=True, exist_ok=True)
    out = {
        "summary": summary,
        "agents": sorted(versions),
        "otherRequests": other,
        "refusedByProxy": refused,
        "emitted": emitted,
        "itemTable": table,
        "requests": requests,
    }
    (capture / f"{summary['name']}.json").write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
