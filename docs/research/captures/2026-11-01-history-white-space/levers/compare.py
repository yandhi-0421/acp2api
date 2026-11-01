"""Compare each emitted value with the same value in each later request.

  python3 compare.py <capture-dir>

It reads only the redacted captures, and it writes <capture-dir>/results.json.
It runs the current rule and an exact rule on the same data.
"""
import json
import pathlib
import re
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cases
import port

WS = "".join(sorted(port.WHITE))


def transformation(before, after):
    if before == after:
        return "identical"
    if after == before.rstrip(WS):
        return "trailing white space removed"
    if after == before.lstrip(WS):
        return "leading white space removed"
    if after == before.strip(WS):
        return "leading and trailing white space removed"
    if after == before.replace("\r\n", "\n"):
        return "CRLF changed to LF"
    if after == before.replace("\r\n", "\n").replace("\r", "\n"):
        return "CRLF and CR changed to LF"
    if after.strip(WS) == before.replace("\r\n", "\n").strip(WS):
        return "CRLF changed to LF and outer white space removed"
    if after == "\n".join(line.rstrip(" \t") for line in before.split("\n")):
        return "trailing spaces and tabs removed from each line"
    return f"other: {before!r} -> {after!r}"


def outcome(before, after):
    return {
        "transformation": transformation(before, after),
        "exactRuleMatches": before == after,
        "currentRuleMatches": port.norm(before) == port.norm(after),
    }


def leaves(value, path="$"):
    """Each string and number with its path, in JSON order."""
    if isinstance(value, dict):
        out = []
        for key, inner in value.items():
            out += leaves(inner, f"{path}.{key}")
        return out
    if isinstance(value, list):
        out = []
        for index, inner in enumerate(value):
            out += leaves(inner, f"{path}[{index}]")
        return out
    return [(path, value)]


def keys(value, path="$"):
    out = []
    if isinstance(value, dict):
        out.append((path, list(value)))
        for key, inner in value.items():
            out += keys(inner, f"{path}.{key}")
    elif isinstance(value, list):
        for index, inner in enumerate(value):
            out += keys(inner, f"{path}[{index}]")
    return out


def argument_outcome(raw, later, custom):
    """acp2api emits compact JSON. Thus raw or its compact form can match."""
    if custom:
        return {"form": "free text", **outcome(raw, later)}
    before = json.loads(raw)
    after = json.loads(later)
    expected = port.compact(before)
    left = dict(leaves(before))
    right = dict(leaves(after))
    strings = {}
    numbers = {}
    for path in sorted(set(left) | set(right)):
        a, b = left.get(path, "<absent>"), right.get(path, "<absent>")
        if type(a) is type(b) and a == b:
            continue
        target = strings if isinstance(a, str) or isinstance(b, str) else numbers
        target[path] = {"before": a, "after": b}
    return {
        "rawBytesIdentical": raw == later,
        "compactFormIdentical": expected == later or port.compact(after) == expected,
        "keyOrderChanged": keys(before) != keys(after),
        "stringValueChanges": strings,
        "numberChanges": numbers,
        "exactRuleMatches": later in (raw, expected),
        "currentRuleMatches": port.norm(later) in (port.norm(raw), port.norm(expected)),
    }


def group(rows):
    """Keep one row for each distinct result, with the requests that show it."""
    grouped = {}
    for request, row in rows:
        key = json.dumps(row, sort_keys=True, ensure_ascii=False)
        grouped.setdefault(key, {"result": row, "requests": []})["requests"].append(request)
    return list(grouped.values())


def assistant_parts(request):
    for index, item in enumerate(request["items"]):
        if item["kind"] == "message" and item.get("role") == "assistant":
            yield index, item["parts"]


def later_requests(run, step):
    index = next(i for i, request in enumerate(run["requests"]) if request["step"] == step)
    return index, run["requests"][index + 1 :]


def text_results(run):
    out = {}
    for emitted in run["emitted"]:
        if emitted["kind"] != "text":
            continue
        step = emitted["step"]
        if step in ("Z99", "side"):
            continue
        name = f"{step}.{emitted['block']}"
        index, later = later_requests(run, step)
        rows = []
        for request in later:
            found = None
            if step == "A17":
                found = position_of_a17(request)
            else:
                marker = re.search(r"[AB]\d\d", emitted["text"]).group(0)
                wanted = [
                    part
                    for _, parts in assistant_parts(request)
                    for part in parts
                    if marker in part.get("text", "")
                ]
                if step == "A16":
                    wanted = [part for part in wanted if ("first" in part["text"]) == ("first" in emitted["text"])]
                found = wanted[0] if wanted else None
            if isinstance(found, str):
                rows.append((request["seq"], {"transformation": found}))
            elif found is None:
                rows.append((request["seq"], {"transformation": "not found in this request"}))
            else:
                rows.append((request["seq"], outcome(emitted["text"], found["text"])))
        out[name] = {"emitted": emitted["text"], "laterRequests": len(later), "observed": group(rows)}
    return out


def position_of_a17(request):
    items = request["items"]
    for index, item in enumerate(items):
        if item["kind"] == "message" and item.get("role") == "user":
            if any("P17" in part.get("text", "") for part in item["parts"]):
                following = items[index + 1] if index + 1 < len(items) else None
                if following is None:
                    return "the reply is absent: nothing follows the prompt"
                if following.get("role") == "assistant" and following["parts"]:
                    first = following["parts"][0]
                    if not first.get("text", "x").strip(WS):
                        return first
                return "the reply is absent: the next item is not this reply"
    return "the prompt is absent"


def call_results(run):
    out = {}
    for emitted in run["emitted"]:
        if emitted["kind"] != "call":
            continue
        _, later = later_requests(run, emitted["step"])
        rows = []
        for request in later:
            found = [item for item in request["items"] if item["kind"] == "call" and item["id"] == emitted["id"]]
            if not found:
                rows.append((request["seq"], {"transformation": "not found in this request"}))
                continue
            row = argument_outcome(emitted["raw"], found[0]["arguments"], emitted.get("custom", False))
            row["name"] = found[0]["name"]
            rows.append((request["seq"], row))
        out[emitted["step"]] = {"tool": emitted["name"], "emitted": emitted["raw"], "laterRequests": len(later), "observed": group(rows)}
    return out


def result_results(run):
    out = {}
    for emitted in run["emitted"]:
        if emitted["kind"] != "call":
            continue
        _, later = later_requests(run, emitted["step"])
        rows = []
        first = None
        for request in later:
            found = [item for item in request["items"] if item["kind"] == "result" and item["id"] == emitted["id"]]
            if not found:
                continue
            item = found[0]
            texts = [part.get("text") for part in item["parts"]]
            shas = [part["sha"] for part in item["parts"]]
            if first is None:
                first = {"seq": request["seq"], "shas": shas, "texts": texts, "error": item["error"]}
            rows.append((request["seq"], {"sameAsFirstRequest": shas == first["shas"] and item["error"] == first["error"]}))
        case = None
        if emitted["step"] in cases.STEP_ARGS:
            case = json.loads(emitted["raw"])["result_case"]
        entry = {"first": first, "stability": group(rows)}
        if case:
            value = cases.RESULTS[case]
            expected = value if isinstance(value, list) else [value]
            entry["toolEmitted"] = expected
            if first:
                entry["clientForm"] = [client_form(value, first["texts"]) for value in expected]
                entry["partCounts"] = {"emitted": len(expected), "request": len(first["texts"])}
        out[emitted["step"]] = entry
    return out


def client_form(value, texts):
    """The first form of a tool result in a request. The client is its author."""
    kept = [text for text in texts if text is not None]
    if value.strip(WS):
        found = [text for text in kept if value.strip(WS) in text]
    else:
        found = [text for text in kept if text == value or "no output" in text]
    if not found:
        return {"emitted": value, "request": None}
    after = found[0]
    if "<system-reminder>" in after:
        after = after.split("<system-reminder>")[0] + "<system-reminder...>"
    return {"emitted": value, "request": after, "identical": after == value}


def keys_of(item, rule):
    field = "sha" if rule == "exact" else "normSha"
    if item["kind"] == "call":
        return ("call", item["name"], item[field])
    if item["kind"] == "result":
        return ("result", item["id"], item.get("error"), tuple(part[field] for part in item["parts"]))
    return (item["kind"], item.get("role"), item.get("name"), tuple(part[field] for part in item["parts"]))


def reply_items(run, step):
    items = []
    texts = [e for e in run["emitted"] if e["step"] == step and e["kind"] == "text"]
    calls = [e for e in run["emitted"] if e["step"] == step and e["kind"] == "call"]
    parts = [{"sha": port.sha(e["text"]), "normSha": port.sha(port.norm(e["text"]))} for e in texts]
    if run["protocol"] == "responses":
        # The fake sends one message item for each text block.
        items.extend({"kind": "message", "role": "assistant", "parts": [part], "block": index} for index, part in enumerate(parts))
    elif parts:
        items.append({"kind": "message", "role": "assistant", "parts": parts})
    if not parts and not calls:
        items.append({"kind": "message", "role": "assistant", "parts": []})
    for call in calls:
        # Responses and Chat Completions carry the arguments as a string.
        if call.get("custom") or run["protocol"] in ("responses", "chat"):
            form = call["raw"]
        else:
            form = port.compact(json.loads(call["raw"]))
        items.append({"kind": "call", "id": call["id"], "name": call["name"], "sha": port.sha(form), "normSha": port.sha(port.norm(form))})
    return items


def describe(item):
    if item is None:
        return None
    out = {key: item[key] for key in ("kind", "role", "id", "name") if key in item}
    texts = [part.get("text") for part in item.get("parts", []) if "text" in part]
    if texts:
        out["text"] = texts
    elif item.get("parts"):
        out["len"] = [part.get("len") for part in item["parts"]]
    if "arguments" in item:
        out["arguments"] = item["arguments"]
    return out


def prefix_results(run):
    rows = []
    requests = run["requests"]
    for current, following in zip(requests, requests[1:]):
        record = current["items"] + reply_items(run, current["step"] or "side")
        later = following["items"]
        row = {
            "from": current["seq"],
            "to": following["seq"],
            "step": current["step"],
            "systemExact": current["systemSha"] == following["systemSha"],
            "systemCurrent": current["systemNormSha"] == following["systemNormSha"],
            "toolSetExact": current["toolSetSha"] == following["toolSetSha"],
        }
        for rule in ("exact", "current"):
            mismatch = None
            for index, item in enumerate(record):
                if index >= len(later) or keys_of(item, rule) != keys_of(later[index], rule):
                    mismatch = index
                    break
            row[f"prefix_{rule}"] = mismatch is None
            if mismatch is None:
                row[f"rest_{rule}"] = port.continues_after(later, len(record))
            elif rule == "exact":
                row["firstMismatch"] = {
                    "index": mismatch,
                    "record": describe(record[mismatch]),
                    "later": describe(later[mismatch] if mismatch < len(later) else None),
                }
        rows.append(row)
    return rows


def text_matches(emitted, part, form):
    if form == "exact":
        return port.sha(emitted) == part["sha"]
    if form == "current":
        return port.sha(port.norm(emitted)) == part["normSha"]
    return port.sha(port.clean(emitted)) == part["cleanSha"]


def continues(run, current, following, earlier, reply_text, reply_call):
    """One rule: the form for the items of the request and for the last reply."""
    if current[f"system{'Sha' if earlier == 'exact' else 'NormSha'}"] != following[f"system{'Sha' if earlier == 'exact' else 'NormSha'}"]:
        return False
    if current["toolSetSha"] != following["toolSetSha"]:
        return False
    later = following["items"]
    size = len(current["items"])
    if len(later) < size:
        return False
    if any(keys_of(a, earlier) != keys_of(b, earlier) for a, b in zip(current["items"], later)):
        return False
    step = current["step"] or "side"
    index = size
    for item in reply_items(run, step):
        if index >= len(later):
            return False
        other = later[index]
        if item["kind"] == "call":
            if other["kind"] != "call" or other["id"] != item["id"] or other["name"] != item["name"]:
                return False
            if reply_call != "id" and item["sha" if reply_call == "exact" else "normSha"] != other["sha" if reply_call == "exact" else "normSha"]:
                return False
        else:
            texts = [e["text"] for e in run["emitted"] if e["step"] == step and e["kind"] == "text"]
            block = item.get("block", 0)
            if other["kind"] != "message" or other.get("role") != "assistant":
                return False
            if run["protocol"] == "responses":
                pairs = [(texts[block], other["parts"][0])] if other["parts"] else []
            else:
                pairs = list(zip(texts, other["parts"]))
                if len(texts) != len(other["parts"]):
                    return False
            if not pairs or not all(text_matches(text, part, reply_text) for text, part in pairs):
                return False
        index += 1
    return port.continues_after(later, index)


RULES = {
    "current rule": ("current", "current", "current"),
    "exact for all": ("exact", "exact", "exact"),
    "issue option: exact, and the current normalization for the last reply text": ("exact", "current", "exact"),
    "exact, and the clean form for the last reply text": ("exact", "clean", "exact"),
    "exact, the clean form for the last reply text, and call ID and name for the last tool call": ("exact", "clean", "id"),
}


def rule_table(run):
    table = {}
    requests = run["requests"]
    for name, (earlier, reply_text, reply_call) in RULES.items():
        missed = []
        for current, following in zip(requests, requests[1:]):
            if not continues(run, current, following, earlier, reply_text, reply_call):
                missed.append(current["step"])
        table[name] = {"pairs": len(requests) - 1, "continued": len(requests) - 1 - len(missed), "rebuildAfter": missed}
    return table


def main(capture):
    capture = pathlib.Path(capture)
    results = {}
    for path in sorted(capture.glob("*.json")):
        if path.name == "results.json":
            continue
        run = json.loads(path.read_text())
        for request in run["requests"]:
            request["items"] = [run["itemTable"][reference] for reference in request["items"]]
        run["protocol"] = {"codex": "responses", "claude": "messages", "sillytavern": "chat"}[run["summary"]["client"]]
        prefix = prefix_results(run)
        results[path.stem] = {
            "summary": run["summary"],
            "agents": run["agents"],
            "otherRequests": run["otherRequests"],
            "refusedByProxy": run["refusedByProxy"],
            "assistantText": text_results(run),
            "toolArguments": call_results(run),
            "toolResults": result_results(run),
            "recordCheck": prefix,
            "ruleTable": rule_table(run),
        }
        print(f"== {path.stem}: {json.dumps(run['summary'])}")
        print("  pairs", len(prefix), "prefix exact ok", sum(r["prefix_exact"] for r in prefix), "prefix current ok", sum(r["prefix_current"] for r in prefix), "system same", sum(r["systemExact"] for r in prefix), "tools same", sum(r["toolSetExact"] for r in prefix))
        for kind in ("assistantText", "toolArguments"):
            for name, entry in results[path.stem][kind].items():
                for seen in entry["observed"]:
                    row = seen["result"]
                    mark = row.get("transformation") or (
                        "strings " + ("changed" if row.get("stringValueChanges") else "same")
                        + " keys " + ("moved" if row.get("keyOrderChanged") else "same")
                        + " numbers " + ("changed" if row.get("numberChanges") else "same")
                        + " raw " + ("same" if row.get("rawBytesIdentical") else "differs")
                    )
                    print(f"  {kind[:6]} {name:7} n={len(seen['requests']):2} {mark[:110]}")
    (capture / "results.json").write_text(json.dumps(results, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main(sys.argv[1])
