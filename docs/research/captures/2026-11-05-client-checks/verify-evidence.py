"""Check public SC-003 evidence, links and scans; optionally compare private captures."""
import argparse
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import re
from urllib.parse import unquote

HERE = Path(__file__).resolve().parent
NOTE = HERE.parents[1] / "2026-11-05-client-checks.md"


def instruction_hit(data, signatures):
    for length, hashes in signatures.items():
        for offset in range(len(data) - length + 1):
            if hashlib.sha256(data[offset:offset + length]).digest() in hashes:
                return True
    return False


def strings(value):
    if isinstance(value, dict):
        for item in value.values():
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)
    elif isinstance(value, str):
        yield value


def check_projection(original, public, field=""):
    if isinstance(public, dict) and "sha256" in public and "bytes" in public:
        data = original.encode() if isinstance(original, str) else json.dumps(
            original, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
        assert public["sha256"] == hashlib.sha256(data).hexdigest() and public["bytes"] == len(data)
        if isinstance(original, dict):
            assert public["field_names"] == list(original)
        if isinstance(original, str):
            assert public.get("test_markers", []) == sorted(set(re.findall(r"SC239-[A-Z]+", original)))
        return 1
    if isinstance(original, dict):
        assert set(original) == set(public) - {"_definition"}
        count = check_projection(original, public["_definition"]) if "_definition" in public else 0
        return count + sum(check_projection(value, public[key], key) for key, value in original.items())
    if isinstance(original, list):
        assert len(original) == len(public)
        return sum(check_projection(a, b, field) for a, b in zip(original, public))
    assert original == public
    if isinstance(original, str) and field in ["text", "content", "instructions", "system", "description"]:
        assert original in ["SC239-OK", "SC239-PARTIAL", "SC239-MAIN", "SC239-SUBAGENT"], "unhashed client text"
    return 0


def check_private(raw_root, records):
    fingerprints = 0
    for identity, cases in records:
        root = raw_root / identity["source"]
        raw_identity = json.loads((root / "identity.json").read_text())
        assert raw_identity["head"] == identity["head"]
        hashes = {Path(path).name: digest for path, digest in raw_identity["lever_sha256"].items()}
        assert all(hashes[name] == digest for name, digest in identity["lever_sha256"].items())
        reviewed = json.loads((HERE / f"native-run-{identity['run']}" / "results.json").read_text())
        for result in reviewed:
            original = json.loads((root / result["check"] / "result.json").read_text())
            assert result["recorded_status"] == original["status"] and result["recorded_error"] == original.get("error")
            assert all(result.get(key) == original.get(key) for key in ["seconds", "requests", "client_exit", "client"])
        for case in cases:
            source = root / case["case"]
            for name, expected in case["source_files"].items():
                path = source / name
                assert path.stat().st_size <= 4 * 1024 * 1024, "private input cap"
                data = path.read_bytes()
                assert expected == dict(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
            original = json.loads((source / "request-index.json").read_text())
            assert len(original) == len(case["requests"])
            for a, b in zip(original, case["requests"]):
                assert {k: v for k, v in a.items() if k != "body"} == {
                    k: v for k, v in b.items() if k not in ["body", "direct_user_markers"]}
                fingerprints += check_projection(a["body"], b["body"])
                texts = []
                for message in a["body"].get("messages", a["body"].get("input", [])):
                    if message.get("role") == "user":
                        content = message["content"]
                        texts.extend([content] if isinstance(content, str) else [
                            block["text"] for block in content if block["type"] in ["text", "input_text"]])
                markers = {match[0] for text in texts for line in text.splitlines()
                           if (match := re.match(r"SC239-[A-Z]+", line))}
                assert b["direct_user_markers"] == sorted(markers)
            wire = [json.loads(line) for line in (source / "wire.ndjson").read_text().splitlines()]
            assert len(wire) == len(case["wire"])
            for a, b in zip(wire, case["wire"]):
                assert {k: v for k, v in a.items() if k not in ["frame", "body"]} == {
                    k: v for k, v in b.items() if k not in ["frame", "body", "data"]}
                for key in ["frame", "body"]:
                    if key in a:
                        fingerprints += check_projection(a[key], b[key])
                        payload = next((line[6:] for line in a[key].splitlines() if line.startswith("data: ")), "") if key == "frame" else a[key]
                        if payload:
                            fingerprints += check_projection(json.loads(payload), b["data"])
            count = 0
            configs = []
            for number, line in enumerate((source / "host.log").read_text().splitlines(), 1):
                if line.startswith("SC239 "):
                    event = json.loads(line[6:])
                    if event["kind"] == "hub" and event["event"]["kind"] == "got":
                        call = json.loads(event["event"]["line"])
                        count += call.get("method") == "session/prompt"
                        if call.get("method") == "session/set_config_option":
                            configs.append(dict(source_line=number, wall_ms=event["wall_ms"], **call))
            assert count == case["acp_prompt_count"]
            assert configs == case["acp_config"]
            for error in case["client_errors"]:
                line = (source / "client.log").read_text().splitlines()[error["source_line"] - 1]
                record = json.loads(line) if line.startswith("{") else None
                expected = record["message"] if record and record["type"] == "error" else record["result"] if record else line
                assert error["text"] == expected and len(error["text"]) <= 512
            result = json.loads((source / "result.json").read_text())
            assert case["recorded_status"] == result["status"] and case["client_exit"] == result.get("client_exit")
    return fingerprints


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-root", type=Path, help="optional private 239-native folder; never copied")
    parser.add_argument("--output", type=Path, help="optional JSON result path")
    args = parser.parse_args()
    files = sorted([NOTE, *[path for path in HERE.rglob("*") if path.is_file()]])
    assert sum(path.stat().st_size for path in files) < 2_000_000, "public evidence size cap"
    signatures = {}
    for signature in json.loads((HERE / "prompt-signatures.json").read_text()):
        signatures.setdefault(signature["bytes"], set()).add(bytes.fromhex(signature["sha256"]))
    private_hits = []
    prompt_hits = []
    credential_hits = []
    for path in files:
        text = path.read_text()
        label = str(path.relative_to(HERE)) if path.is_relative_to(HERE) else path.name
        if Path.home().name.casefold() in text.casefold() or str(Path.home()).casefold() in text.casefold():
            private_hits.append(label)
        for email in re.findall(r"[\w.+-]+(?:@|%40)[\w.-]+", text, re.I):
            if not email.endswith("example.invalid"):
                private_hits.append(label)
        if re.search(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|\bsk-[A-Za-z0-9_-]{20,}|\bgh[pousr]_[A-Za-z0-9]{20,}|\bAIza[A-Za-z0-9_-]{30,}|\b\d+-[A-Za-z0-9_-]+\.apps\.googleusercontent\.com\b|client_id(?:=|%3d)[A-Za-z0-9._~-]{8,}", text, re.I):
            credential_hits.append(label)
        candidates = [text]
        if path.suffix == ".json":
            candidates += list(strings(json.loads(text)))
        elif path.suffix == ".ndjson":
            for line in text.splitlines():
                candidates += list(strings(json.loads(line)))
        elif path.suffix == ".py":
            candidates += [node.value for node in ast.walk(ast.parse(text)) if isinstance(node, ast.Constant) and isinstance(node.value, str)]
        if any(instruction_hit(value.encode(), signatures) for value in candidates):
            prompt_hits.append(label)
    assert not private_hits and not credential_hits and not prompt_hits, dict(
        personal_data=private_hits, credentials=credential_hits, client_instructions=prompt_hits)
    links = 0
    for doc in [NOTE, HERE / "README.md"]:
        for title, href in re.findall(r"\[([^\]]+)\]\(([^)]+)\)", doc.read_text()):
            if re.match(r"[a-z]+://", href):
                continue
            path, _, anchor = unquote(href).partition("#")
            target = (doc.parent / path).resolve() if path else doc
            assert target.is_file(), (doc.name, title, href)
            if re.fullmatch(r"L\d+", anchor):
                assert 1 <= int(anchor[1:]) <= len(target.read_text().splitlines()), href
            elif anchor:
                slugs = [re.sub(r"[^\w -]", "", heading.lower()).replace(" ", "-")
                         for heading in re.findall(r"^#+ (.+)$", target.read_text(), re.M)]
                assert anchor in slugs, href
            links += 1
    spec = importlib.util.spec_from_file_location("reviewed_native", HERE / "summarize-rounds.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    outputs, counts = module.reviewed_results()
    for path, value in outputs.items():
        assert path.read_text() == json.dumps(value, indent=2, ensure_ascii=False) + "\n", path.name
    rows = json.loads((HERE / "latest-results.json").read_text())
    assert re.findall(r"^\| `([^`]+)` \|", NOTE.read_text(), re.M) == [row["check"] for row in rows]
    for row in rows:
        path, line = row["source"].rsplit(":", 1)
        assert f'"status": "{row["status"]}"' in (HERE / path).read_text().splitlines()[int(line) - 1]
    records = []
    for number in [1, 2, 3]:
        run = list(map(json.loads, (HERE / f"native-run-{number}/evidence.ndjson").read_text().splitlines()))
        records.append((run[0], run[1:]))
    child = next(row for row in records[2][1] if row["case"] == "07-subagent")
    error = next(event for event in child["wire"] if event["id"] == 3 and event.get("event") == "error")
    early = next(request for request in child["requests"] if request["id"] == 6)
    assert 29.97 < early["at"] - error["at"] < 30 and early["status"] == 503
    assert early["response_headers"]["retry-after"] == "1"
    result = dict(files_scanned=len(files), local_links=links, personal_data_hits=0, credential_hits=0,
                  client_instruction_hits=0, prompt_signatures=sum(map(len, signatures.values())),
                  latest=counts["latest"], raw_fingerprints_verified=check_private(args.raw_root, records) if args.raw_root else None)
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()
