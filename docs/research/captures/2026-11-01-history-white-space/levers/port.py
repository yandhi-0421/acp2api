"""A Python copy of the parts of the acp2api match rule that the probe compares.

Source at commit 684d3c3:
- crates/acp2api/src/conversation.rs: ItemHasher::text, item_hash, is_continued_by,
  continues_after, user_suffix, tool_set_hash.
- crates/acp2api/src/dialect/content.rs: parse, messages_blocks, responses_item,
  result_parts.
The product does not run in this probe.
"""
import hashlib
import json

# char::is_whitespace of Rust: the Unicode property White_Space.
WHITE = set(
    "\t\n\x0b\x0c\r \x85\xa0        "
    "        　"
)


def norm(text):
    out = []
    space = False
    for ch in text:
        if ch in WHITE:
            if not space:
                out.append(" ")
            space = True
        else:
            out.append(ch)
            space = False
    return "".join(out)


LINE_WHITE = "".join(sorted(WHITE - {"\n", "\r"}))


def clean(text):
    """A candidate compatibility form for the reply text of acp2api.

    It removes each CR, the white space at each line end, and the white space
    at both ends of the text. It keeps indentation and runs of spaces inside a
    line. It is the set of changes that SillyTavern 1.18.0 made in this probe.
    """
    lines = [line.rstrip(LINE_WHITE) for line in text.replace("\r", "").split("\n")]
    return "\n".join(lines).strip("".join(sorted(WHITE)))


def sha(text):
    return hashlib.sha256(text.encode("utf-8", "surrogatepass")).hexdigest()[:16]


def compact(value):
    """serde_json::Value::to_string with preserve_order."""
    return json.dumps(value, separators=(",", ":"), ensure_ascii=False)


def message_parts(content):
    if isinstance(content, str):
        return [["text", content]]
    parts = []
    for block in content or []:
        parts.extend(block_part(block))
    return parts


def block_part(block):
    kind = block.get("type")
    if kind in ("thinking", "redacted_thinking", "tool_addition"):
        return []
    if kind in ("text", "input_text", "output_text"):
        return [["text", block.get("text", "")]]
    return [[f"other:{kind}", json.dumps(block, sort_keys=True)]]


def messages_items(body):
    system = body.get("system")
    texts = []
    if isinstance(system, str):
        texts = [system]
    elif isinstance(system, list):
        texts = [block.get("text", "") for block in system]
        if texts and texts[0].startswith("x-anthropic-billing-header:"):
            texts = texts[1:]
    items = []
    for message in body.get("messages", []):
        role, content = message["role"], message.get("content")
        tools = isinstance(content, list) and any(
            block.get("type") in ("tool_use", "tool_result") for block in content
        )
        if not tools:
            items.append({"kind": "message", "role": role, "parts": message_parts(content)})
            continue
        pending = []
        for block in content:
            kind = block.get("type")
            if kind == "tool_use":
                if pending:
                    items.append({"kind": "message", "role": role, "parts": pending})
                    pending = []
                items.append(
                    {
                        "kind": "call",
                        "id": block["id"],
                        "name": block["name"],
                        "arguments": compact(block["input"]),
                    }
                )
            elif kind == "tool_result":
                if pending:
                    items.append({"kind": "message", "role": role, "parts": pending})
                    pending = []
                items.append(
                    {
                        "kind": "result",
                        "id": block["tool_use_id"],
                        "error": bool(block.get("is_error", False)),
                        "parts": message_parts(block.get("content")),
                    }
                )
            else:
                pending.extend(block_part(block))
        if pending:
            items.append({"kind": "message", "role": role, "parts": pending})
    return "\n\n".join(texts), items


def responses_items(body):
    system = []
    if body.get("instructions") is not None:
        system.append(body["instructions"])
    items = []
    at_start = True
    source = body.get("input", [])
    if isinstance(source, str):
        source = [{"role": "user", "content": source}]
    for item in source:
        kind = item.get("type")
        if kind == "reasoning":
            continue
        if kind in ("function_call", "custom_tool_call"):
            at_start = False
            field = "arguments" if kind == "function_call" else "input"
            items.append(
                {
                    "kind": "call",
                    "id": item["call_id"],
                    "name": item["name"],
                    "arguments": item[field],
                    "wire": kind,
                }
            )
            continue
        if kind in ("function_call_output", "custom_tool_call_output"):
            at_start = False
            items.append(
                {
                    "kind": "result",
                    "id": item["call_id"],
                    "error": False,
                    "parts": message_parts(item.get("output")),
                }
            )
            continue
        if kind not in (None, "message"):
            at_start = False
            items.append({"kind": f"other:{kind}", "role": "other", "parts": [["other", json.dumps(item, sort_keys=True)]]})
            continue
        role = item["role"]
        if at_start and role in ("system", "developer"):
            system.extend(text for tag, text in message_parts(item.get("content")))
            continue
        at_start = False
        items.append({"kind": "message", "role": role, "parts": message_parts(item.get("content"))})
    return "\n\n".join(system), items


def chat_items(body):
    system = []
    items = []
    at_start = True
    for message in body.get("messages", []):
        role = message["role"]
        if role == "tool":
            at_start = False
            items.append({"kind": "result", "id": message["tool_call_id"], "error": False, "parts": message_parts(message.get("content"))})
            continue
        if at_start and role in ("system", "developer"):
            texts = [text for tag, text in message_parts(message.get("content"))]
            label = f"[{role}: {message['name']}]\n" if message.get("name") else ""
            system.append(label + "\n\n".join(texts))
            continue
        at_start = False
        calls = message.get("tool_calls")
        content = message.get("content")
        if not calls or content not in (None, ""):
            entry = {"kind": "message", "role": role, "parts": message_parts(content)}
            if message.get("name"):
                entry["name"] = message["name"]
            items.append(entry)
        for call in calls or []:
            items.append({"kind": "call", "id": call["id"], "name": call["function"]["name"], "arguments": call["function"]["arguments"]})
    return "\n\n".join(system), items


def item_key(item, rule):
    """The fields that item_hash reads. rule is exact or current."""
    text = norm if rule == "current" else (lambda value: value)
    if item["kind"] == "call":
        return ("call", text(item["name"]), text(item["arguments"]))
    if item["kind"] == "result":
        return ("result", item["id"], item["error"], tuple((tag, text(body)) for tag, body in item["parts"]))
    return (item["kind"], item.get("role"), tuple((tag, text(body)) for tag, body in item["parts"]))


def continues_after(items, length):
    if length == 0 or length > len(items):
        return False
    last, following = items[length - 1], items[length:]
    def user_suffix(rest):
        return bool(rest) and all(
            entry["kind"] == "message" and entry.get("role") in ("user", "system", "developer")
            for entry in rest
        )
    if last["kind"] == "call":
        if not following or following[0]["kind"] != "result" or following[0]["id"] != last["id"]:
            return False
        return len(following) == 1 or user_suffix(following[1:])
    return user_suffix(following)
