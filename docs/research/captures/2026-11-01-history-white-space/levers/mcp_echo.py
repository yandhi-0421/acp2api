"""Stdio MCP server of the probe. It returns one fixed result for each case.

Limits: MAX_CALLS messages and MAX_SECONDS of lifetime. It writes no log file.
"""
import json
import os
import pathlib
import sys
import threading
import time

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import cases

MAX_CALLS = 200
MAX_SECONDS = 300


def lifetime():
    time.sleep(MAX_SECONDS)
    os._exit(3)


threading.Thread(target=lifetime, daemon=True).start()

TOOL = {
    "name": "probe_echo",
    "description": "Return a  fixed   result.\n\n  Second line\twith a tab.  \n",
    "inputSchema": {
        "type": "object",
        "properties": {
            "result_case": {"type": "string", "description": "  case  id  "},
            "text": {"type": "string"},
            "note": {"type": "string"},
            "n": {"type": "number"},
            "m": {"type": "number"},
            "big": {"type": "number"},
            "obj": {"type": "object"},
            "list": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["result_case"],
    },
}


def reply(request_id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}) + "\n")
    sys.stdout.flush()


calls = 0
for line in sys.stdin:
    calls += 1
    if calls > MAX_CALLS:
        break
    try:
        message = json.loads(line)
    except ValueError:
        continue
    method = message.get("method")
    request_id = message.get("id")
    if method == "initialize":
        version = message.get("params", {}).get("protocolVersion", "2025-06-18")
        reply(request_id, {"protocolVersion": version, "capabilities": {"tools": {}}, "serverInfo": {"name": "probe", "version": "1.0"}})
    elif method == "tools/list":
        reply(request_id, {"tools": [TOOL]})
    elif method == "tools/call":
        case = message["params"].get("arguments", {}).get("result_case", "R01")
        value = cases.RESULTS.get(case, "unknown case")
        texts = value if isinstance(value, list) else [value]
        reply(request_id, {"content": [{"type": "text", "text": text} for text in texts], "isError": case == "R08"})
    elif request_id is not None:
        reply(request_id, {})
