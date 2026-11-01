"""Fake Anthropic Messages and OpenAI Responses endpoint for issue 173.

Limits: --max-requests, --max-seconds, --max-log-bytes. After a limit, each
request gets HTTP 400 and the log stops. No request goes to a real model.
"""
import argparse
import gzip
import json
import os
import pathlib
import re
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import cases

parser = argparse.ArgumentParser()
parser.add_argument("--port", type=int, required=True)
parser.add_argument("--run-dir", required=True)
parser.add_argument("--client", required=True)
parser.add_argument("--work", required=True)
parser.add_argument("--max-requests", type=int, default=120)
parser.add_argument("--max-seconds", type=int, default=300)
parser.add_argument("--max-log-bytes", type=int, default=30_000_000)
parser.add_argument("--max-body-bytes", type=int, default=4_000_000)
parser.add_argument("--min-tools", type=int, default=4)
parser.add_argument("--skip", default="")
args = parser.parse_args()

RUN = pathlib.Path(args.run_dir)
LOG = RUN / "requests.jsonl"
EMITTED = RUN / "emitted.jsonl"
STEPS = cases.script(args.client, [part for part in args.skip.split(",") if part])
lock = threading.Lock()
state = {
    "requests": 0,
    "main": 0,
    "steps_done": 0,
    "active": 0,
    "log_bytes": 0,
    "stopped": "",
    "seen_tools": [],
}
HEADERS = ("content-type", "content-encoding", "user-agent", "anthropic-beta", "anthropic-version", "x-claude-code-session-id")
TOOL_NAMES = {}


def stop(reason):
    if not state["stopped"]:
        state["stopped"] = reason


def lifetime():
    time.sleep(args.max_seconds)
    with lock:
        stop("max-seconds")
    os._exit(3)


threading.Thread(target=lifetime, daemon=True).start()


def write_log(record):
    line = json.dumps(record, ensure_ascii=False) + "\n"
    size = len(line.encode("utf-8", "surrogatepass"))
    if state["log_bytes"] + size > args.max_log_bytes:
        stop("max-log-bytes")
        return
    state["log_bytes"] += size
    with LOG.open("a", encoding="utf-8", errors="surrogatepass") as log:
        log.write(line)


def resolve_tool(block, body):
    """Map a step tool to the name that this client offers."""
    wanted = block["tool"]
    if wanted == "probe":
        return probe_tool(body)
    if wanted == "shell":
        for tool in tool_list(body):
            if tool.get("name") in ("exec_command", "shell_command", "shell"):
                return tool["name"], None
    return wanted, None


def tool_list(body):
    out = []
    for tool in body.get("tools") or []:
        if "function" in tool:
            tool = {**tool, "name": tool["function"].get("name")}
        out.append(tool)
        for inner in tool.get("tools") or []:
            out.append({**inner, "_namespace": tool.get("name")})
    return out


def probe_tool(body):
    for tool in tool_list(body):
        name = tool.get("name") or ""
        if "probe_echo" in name:
            return name, tool.get("_namespace")
    return "probe_echo", None


def substitute(raw):
    return raw.replace("{WORK}", args.work)


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def send_json(self, status, value):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/__state":
            with lock:
                self.send_json(200, state)
            return
        with lock:
            state["requests"] += 1
            write_log({"seq": state["requests"], "method": "GET", "path": self.path})
        if self.path.split("?")[0].endswith("/models"):
            self.send_json(200, {"models": [], "data": []})
            return
        self.send_json(404, {"error": {"type": "not_found", "message": "no such path"}})

    def do_POST(self):
        size = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(min(size, args.max_body_bytes))
        with lock:
            state["requests"] += 1
            seq = state["requests"]
            if state["requests"] > args.max_requests:
                stop("max-requests")
            if size > args.max_body_bytes:
                stop("max-body-bytes")
            stopped = state["stopped"]
        if stopped:
            self.send_json(400, {"type": "error", "error": {"type": "invalid_request_error", "message": f"probe limit: {stopped}"}})
            return
        encoding = self.headers.get("content-encoding", "")
        if encoding == "gzip":
            raw = gzip.decompress(raw)
        try:
            body = json.loads(raw)
        except ValueError:
            with lock:
                write_log({"seq": seq, "path": self.path, "undecoded": len(raw), "encoding": encoding})
            self.send_json(400, {"error": {"type": "invalid_request_error", "message": "not JSON"}})
            return
        path = self.path.split("?")[0]
        headers = {name: self.headers.get(name) for name in HEADERS if self.headers.get(name)}
        if path.endswith("/count_tokens"):
            self.send_json(200, {"input_tokens": 1})
            return
        if path.endswith("/chat/completions"):
            main = bool(re.search(r"P\d\d go", json.dumps(body.get("messages", []))))
        else:
            main = len(tool_list(body)) >= args.min_tools
        with lock:
            step = None
            index = None
            if main:
                state["main"] += 1
                index = state["steps_done"]
                if index < len(STEPS):
                    step = STEPS[index]
                    state["steps_done"] = index + 1
                state["active"] += 1
            write_log(
                {
                    "seq": seq,
                    "path": path,
                    "headers": headers,
                    "main": main,
                    "step_index": index,
                    "step": step["id"] if step else None,
                    "body": body,
                }
            )
        if step is None:
            text = "Z98 no script step left" if main else "Probe side reply"
            step = {"id": "side", "blocks": [{"type": "text", "chunks": [text]}]}
        try:
            if path.endswith("/chat/completions"):
                self.reply_chat(step, body, main)
            elif path.endswith("/responses"):
                self.stream_responses(step, body, main)
            else:
                self.stream_messages(step, body, main)
        finally:
            with lock:
                if main:
                    state["active"] -= 1

    def emit(self, step, kind, data):
        with lock:
            with EMITTED.open("a", encoding="utf-8", errors="surrogatepass") as log:
                log.write(json.dumps({"step": step["id"], "kind": kind, **data}, ensure_ascii=False) + "\n")

    def begin_stream(self):
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.send_header("connection", "close")
        self.end_headers()
        self.close_connection = True

    def event(self, name, value):
        data = json.dumps(value, ensure_ascii=False)
        self.wfile.write(f"event: {name}\ndata: {data}\n\n".encode("utf-8", "surrogatepass"))
        self.wfile.flush()

    def stream_messages(self, step, body, main):
        self.begin_stream()
        mid = f"msg_{step['id']}"
        usage = {"input_tokens": 10, "output_tokens": 0}
        self.event("message_start", {"type": "message_start", "message": {"id": mid, "type": "message", "role": "assistant", "model": body.get("model", "m"), "content": [], "stop_reason": None, "stop_sequence": None, "usage": usage}})
        reason = "end_turn"
        for index, block in enumerate(step["blocks"]):
            if block["type"] == "text":
                self.event("content_block_start", {"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}})
                for chunk in block["chunks"]:
                    self.event("content_block_delta", {"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": chunk}})
                self.event("content_block_stop", {"type": "content_block_stop", "index": index})
                if main:
                    self.emit(step, "text", {"block": index, "text": "".join(block["chunks"])})
            else:
                name, _ = resolve_tool(block, body)
                call_id = f"toolu_{step['id']}"
                raw = substitute(block["raw"])
                self.event("content_block_start", {"type": "content_block_start", "index": index, "content_block": {"type": "tool_use", "id": call_id, "name": name, "input": {}}})
                self.event("content_block_delta", {"type": "content_block_delta", "index": index, "delta": {"type": "input_json_delta", "partial_json": raw}})
                self.event("content_block_stop", {"type": "content_block_stop", "index": index})
                reason = "tool_use"
                self.emit(step, "call", {"id": call_id, "name": name, "raw": raw})
        self.event("message_delta", {"type": "message_delta", "delta": {"stop_reason": reason, "stop_sequence": None}, "usage": {"input_tokens": 10, "output_tokens": 5}})
        self.event("message_stop", {"type": "message_stop"})

    def chat_chunk(self, step, body, delta, finish=None):
        value = {"id": f"chatcmpl-{step['id']}", "object": "chat.completion.chunk", "created": 1, "model": body.get("model", "m"), "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
        data = json.dumps(value, ensure_ascii=False)
        self.wfile.write(f"data: {data}\n\n".encode("utf-8", "surrogatepass"))
        self.wfile.flush()

    def reply_chat(self, step, body, main):
        chunks = [chunk for block in step["blocks"] if block["type"] == "text" for chunk in block["chunks"]]
        text = "".join(chunks)
        calls = [block for block in step["blocks"] if block["type"] == "tool"]
        if text and main:
            self.emit(step, "text", {"block": 0, "text": text})
        wire = []
        for block in calls:
            name, _ = resolve_tool(block, body)
            call_id = f"call_{step['id']}"
            raw = substitute(block["raw"])
            wire.append((call_id, name, raw))
            self.emit(step, "call", {"id": call_id, "name": name, "raw": raw})
        finish = "tool_calls" if wire else "stop"
        if not body.get("stream"):
            message = {"role": "assistant", "content": text}
            if wire:
                message["tool_calls"] = [{"id": i, "type": "function", "function": {"name": n, "arguments": r}} for i, n, r in wire]
            self.send_json(200, {"id": f"chatcmpl-{step['id']}", "object": "chat.completion", "created": 1, "model": body.get("model", "m"), "choices": [{"index": 0, "message": message, "finish_reason": finish}], "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
            return
        self.begin_stream()
        self.chat_chunk(step, body, {"role": "assistant", "content": ""})
        for chunk in chunks:
            self.chat_chunk(step, body, {"content": chunk})
        for call_id, name, raw in wire:
            self.chat_chunk(step, body, {"tool_calls": [{"index": 0, "id": call_id, "type": "function", "function": {"name": name, "arguments": ""}}]})
            self.chat_chunk(step, body, {"tool_calls": [{"index": 0, "function": {"arguments": raw}}]})
        self.chat_chunk(step, body, {}, finish)
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def stream_responses(self, step, body, main):
        self.begin_stream()
        rid = f"resp_{step['id']}"
        created = {"id": rid, "object": "response", "status": "in_progress", "output": [], "model": body.get("model", "m")}
        self.event("response.created", {"type": "response.created", "response": created})
        self.event("response.in_progress", {"type": "response.in_progress", "response": created})
        output = []
        for block in step["blocks"]:
            index = len(output)
            if block["type"] == "text":
                item_id = f"msg_{step['id']}_{index}"
                text = "".join(block["chunks"])
                part = {"type": "output_text", "text": text, "annotations": [], "logprobs": []}
                self.event("response.output_item.added", {"type": "response.output_item.added", "output_index": index, "item": {"id": item_id, "type": "message", "status": "in_progress", "role": "assistant", "content": []}})
                self.event("response.content_part.added", {"type": "response.content_part.added", "output_index": index, "content_index": 0, "item_id": item_id, "part": {**part, "text": ""}})
                for chunk in block["chunks"]:
                    self.event("response.output_text.delta", {"type": "response.output_text.delta", "item_id": item_id, "output_index": index, "content_index": 0, "delta": chunk, "logprobs": []})
                self.event("response.output_text.done", {"type": "response.output_text.done", "item_id": item_id, "output_index": index, "content_index": 0, "text": text, "logprobs": []})
                self.event("response.content_part.done", {"type": "response.content_part.done", "item_id": item_id, "output_index": index, "content_index": 0, "part": part})
                item = {"id": item_id, "type": "message", "status": "completed", "role": "assistant", "content": [part]}
                self.event("response.output_item.done", {"type": "response.output_item.done", "output_index": index, "item": item})
                output.append(item)
                if main:
                    self.emit(step, "text", {"block": index, "text": text})
            else:
                name, namespace = resolve_tool(block, body)
                item_id = f"fc_{step['id']}"
                call_id = f"call_{step['id']}"
                custom = block["type"] == "custom"
                raw = block["input"] if custom else substitute(block["raw"])
                field = "input" if custom else "arguments"
                kind = "custom_tool_call" if custom else "function_call"
                item = {"id": item_id, "type": kind, "call_id": call_id, "name": name, field: ""}
                if not custom:
                    item["status"] = "in_progress"
                if namespace:
                    item["namespace"] = namespace
                self.event("response.output_item.added", {"type": "response.output_item.added", "output_index": index, "item": item})
                delta = "response.custom_tool_call_input" if custom else "response.function_call_arguments"
                self.event(delta + ".delta", {"type": delta + ".delta", "item_id": item_id, "output_index": index, "delta": raw})
                self.event(delta + ".done", {"type": delta + ".done", "item_id": item_id, "output_index": index, field: raw})
                done = {**item, field: raw}
                if not custom:
                    done["status"] = "completed"
                self.event("response.output_item.done", {"type": "response.output_item.done", "output_index": index, "item": done})
                output.append(done)
                self.emit(step, "call", {"id": call_id, "name": name, "raw": raw, "custom": custom, "namespace": namespace})
        usage = {"input_tokens": 10, "input_tokens_details": {"cached_tokens": 0}, "output_tokens": 5, "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 15}
        final = {**created, "status": "completed", "output": output, "usage": usage, "error": None}
        self.event("response.completed", {"type": "response.completed", "response": final})


class Server(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 32


Server(("127.0.0.1", args.port), Handler).serve_forever()
