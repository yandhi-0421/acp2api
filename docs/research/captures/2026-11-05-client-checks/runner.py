"""SC-003 research checks. All real API clients require sandbox-exec."""
import argparse
from dataclasses import dataclass
import fcntl
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import http.client
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import traceback

from safety import CHILDREN, LOG_CAP, Log, REAL_HOME, command, redact, save, stop

HERE = Path(__file__).resolve().parent
PROJECT = HERE.parents[3]
BUILD = Path.home() / "acp2api-build"
TARGET = BUILD / "target-239"
VENV = BUILD / "tmp/239-client-checks/sdk-venv"
MODEL = "gemini-3.8-flash"
KEY = "made-up-sc239-key"
REQUEST_CAP = 32
BODY_CAP = 1024 * 1024
TOTAL_SECONDS = 3000
SCRATCH_CAP = 128 * 1024 * 1024
DISK_CAP = 3 * 1024 * 1024 * 1024


@dataclass
class Check:
    name: str
    client: str
    scenario: str = "normal"
    mode: str = "print"
    seconds: int = 90
    pause: int = 0


CHECKS = [
    Check("01-attribution", "claude"),
    Check("02-advisor", "claude", mode="advisor"),
    Check("02-advisor-experimental", "claude", mode="advisor-experimental"),
    Check("03-claude-keep-alive", "claude", "pause", seconds=390, pause=330),
    Check("03-codex-keep-alive", "codex", "pause", seconds=390, pause=330),
    Check("04-effort", "claude", mode="effort"),
    Check("05-claude-lockout", "claude", "lockout", seconds=45),
    Check("05-codex-lockout", "codex", "lockout", seconds=45),
    Check("05-claude-sign-in", "claude", "sign-in", seconds=45),
    Check("05-codex-sign-in", "codex", "sign-in", seconds=45),
    Check("06-claude-retry", "claude", "temporary", seconds=120),
    Check("06-codex-retry", "codex", "temporary", seconds=120),
    Check("07-print-before", "claude", "before", seconds=120),
    Check("07-print-after", "claude", "after", seconds=120),
    Check("07-interactive-before", "claude", "before", "tui", 120),
    Check("07-interactive-after", "claude", "after", "tui", 120),
    Check("07-subagent", "claude", "subagent", "tui", 150),
    Check("08-nonstream-310s", "claude", "nonstream", seconds=390, pause=310),
    Check("09-openai-sdk", "sdk", seconds=45),
]


def local_port():
    with socket.socket() as lease:
        lease.bind(("127.0.0.1", 0))
        return lease.getsockname()[1]


def profile(ports, destination):
    rules = ["(version 1)(allow default)(deny network-outbound)"]
    rules += [f'(allow network-outbound (remote ip "localhost:{port}"))' for port in ports]
    for path in [".codex", ".claude", ".gemini", "Library/Keychains"]:
        rules.append(f'(deny file-read* file-write* (subpath "{REAL_HOME}/{path}"))')
    rules.append(f'(deny file-read* file-write* (literal "{REAL_HOME}/.claude.json"))')
    destination.write_text("\n".join(rules) + "\n")


def clean_env(work, proxy):
    return dict(HOME=str(work / "home"), TMPDIR=str(work / "tmp"),
                PATH=f"{Path(sys.executable).parent}:{Path.home() / '.local/bin'}:/usr/bin:/bin:/usr/sbin:/sbin",
                USER="probe", LOGNAME="probe", LANG="en_US.UTF-8", TERM="xterm-256color",
                SHELL="/bin/zsh", PYTHONDONTWRITEBYTECODE="1", ACP2API_TEST_TIME_FACTOR="2",
                HTTP_PROXY=proxy, HTTPS_PROXY=proxy, http_proxy=proxy, https_proxy=proxy,
                ALL_PROXY=proxy, all_proxy=proxy, NO_PROXY="127.0.0.1,localhost", no_proxy="127.0.0.1,localhost")


def wrap(argv, env, sandbox):
    return ["/usr/bin/sandbox-exec", "-f", str(sandbox), "/usr/bin/env", "-i",
            *[f"{key}={value}" for key, value in env.items()], *map(str, argv)]


class Recorder:
    """A loopback HTTP relay. It never selects an upstream from client input."""
    def __init__(self, output, upstream, seconds, cap=REQUEST_CAP):
        self.output = output
        self.upstream = upstream
        self.seconds = seconds
        self.cap = cap
        self.count = 0
        self.requests = []
        self.events = []
        self.failures = []
        self.lock = threading.Lock()
        self.start = time.monotonic()
        self.request_log = Log(output / "requests.ndjson")
        self.wire_log = Log(output / "wire.ndjson")
        self.proxy_log = Log(output / "proxy.ndjson")
        self.proxy_count = 0
        owner = self

        class Relay(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                self.relay()

            def do_POST(self):
                self.relay()

            def relay(self):
                connection = None
                request = None
                done = threading.Event()
                try:
                    with owner.lock:
                        owner.count += 1
                        ident = owner.count
                    if ident > owner.cap:
                        self.send_response(400)
                        self.send_header("x-should-retry", "false")
                        self.end_headers()
                        self.wfile.write(b'{"type":"error","error":{"type":"invalid_request_error","message":"SC239 request cap"}}')
                        owner.failures.append("request cap")
                        return
                    if self.path == "/__guard":
                        self.send_response(200)
                        self.end_headers()
                        self.wfile.write(b"SC239-GUARD")
                        return
                    self.connection.settimeout(owner.seconds)
                    size = int(self.headers.get("Content-Length", "0"))
                    if not 0 <= size <= BODY_CAP or self.headers.get("Transfer-Encoding"):
                        raise ValueError("request body cap or chunked request")
                    body = self.rfile.read(size)
                    if len(body) != size:
                        raise ValueError("short request body")
                    value = json.loads(body) if body else None
                    now = time.monotonic() - owner.start
                    request = dict(id=ident, at=now, method=self.command, path=self.path, body=value,
                                   headers={key: self.headers[key] for key in ["User-Agent", "anthropic-beta",
                                            "x-stainless-retry-count", "x-stainless-timeout"] if key in self.headers})
                    owner.requests.append(request)
                    owner.request_log.event(**request)
                    connection = http.client.HTTPConnection("127.0.0.1", owner.upstream, timeout=owner.seconds)
                    connection.connect()
                    upstream_socket = connection.sock

                    def watch_disconnect():
                        while not done.wait(0.05):
                            try:
                                readable, _, _ = select.select([self.connection], [], [], 0)
                                if readable and self.connection.recv(1, socket.MSG_PEEK) == b"":
                                    request["client_disconnected_at"] = time.monotonic() - owner.start
                                    owner.wire_log.event(id=ident, at=request["client_disconnected_at"], disconnect="client EOF")
                                    upstream_socket.shutdown(socket.SHUT_RDWR)
                                    return
                            except OSError:
                                return

                    threading.Thread(target=watch_disconnect, daemon=True).start()
                    headers = {key: val for key, val in self.headers.items()
                               if key.lower() not in ["host", "connection", "accept-encoding", "content-length"]}
                    headers["Accept-Encoding"] = "identity"
                    headers["Host"] = f"127.0.0.1:{owner.upstream}"
                    connection.request(self.command, self.path, body, headers)
                    response = connection.getresponse()
                    request["status"] = response.status
                    request["response_headers"] = {key.lower(): val for key, val in response.getheaders()
                                                   if key.lower() not in ["set-cookie"]}
                    owner.wire_log.event(id=ident, at=time.monotonic() - owner.start,
                                         status=response.status, headers=request["response_headers"])
                    self.send_response(response.status)
                    for key, val in response.getheaders():
                        if key.lower() not in ["transfer-encoding", "connection", "server", "date"]:
                            self.send_header(key, val)
                    self.end_headers()
                    buffer = b""
                    result = bytearray()
                    while data := response.read1(8192):
                        at = time.monotonic() - owner.start
                        if len(result) + len(data) > BODY_CAP:
                            raise ValueError("response body cap")
                        result.extend(data)
                        # Parse frames after a complete delimiter, so a split UTF-8 character survives.
                        buffer += data
                        partial = False
                        while b"\n\n" in buffer:
                            frame, buffer = buffer.split(b"\n\n", 1)
                            name = next((line[6:] for line in frame.splitlines() if line.startswith(b"event:")), b"data")
                            event = dict(id=ident, at=at, event=name.decode().strip(), frame=frame.decode("utf-8"))
                            owner.events.append(event)
                            owner.wire_log.event(**event)
                            partial |= name.strip() == b"content_block_delta" and b"SC239-PARTIAL" in frame
                        self.wfile.write(data)
                        self.wfile.flush()
                        if partial:
                            with owner.lock:
                                ack = owner.output / "partial-seen.json"
                                if not ack.exists():
                                    save(ack, dict(id=ident, at=time.monotonic() - owner.start,
                                                   forwarded="SC239-PARTIAL text delta"))
                    request["response"] = result.decode("utf-8")
                    request["ended"] = time.monotonic() - owner.start
                    if not response.getheader("content-type", "").startswith("text/event-stream"):
                        owner.wire_log.event(id=ident, at=request["ended"], body=request["response"])
                    owner.wire_log.event(id=ident, at=request["ended"], end=True)
                except (BrokenPipeError, ConnectionResetError) as error:
                    owner.wire_log.event(id=ident, at=time.monotonic() - owner.start,
                                         disconnect=type(error).__name__)
                except Exception as error:
                    if not request or "client_disconnected_at" not in request:
                        owner.failures.append(str(error))
                finally:
                    done.set()
                    if connection:
                        connection.close()
                    self.close_connection = True

        class Refusal(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def refuse(self):
                owner.proxy_count += 1
                if owner.proxy_count <= 128:
                    owner.proxy_log.event(method=self.command, target=self.path,
                                          at=time.monotonic() - owner.start, refused=403)
                else:
                    owner.failures.append("proxy request cap")
                self.send_response(403)
                self.end_headers()
                self.close_connection = True

            do_CONNECT = do_GET = do_POST = refuse

        self.http = ThreadingHTTPServer(("127.0.0.1", 0), Relay)
        self.proxy = ThreadingHTTPServer(("127.0.0.1", 0), Refusal)
        for server in [self.http, self.proxy]:
            server.daemon_threads = True
            threading.Thread(target=server.serve_forever, daemon=True).start()

    @property
    def port(self):
        return self.http.server_port

    def close(self):
        for server in [self.http, self.proxy]:
            server.shutdown()
            server.server_close()


class Process:
    def __init__(self, argv, output, env, cwd, terminal=False):
        argv = [sys.executable, "-B", str(HERE / "safety.py"), "--limit-exec", *map(str, argv)]
        self.log = Log(output)
        self.tail = ""
        self.failures = []
        self.master = None
        if terminal:
            self.master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 160, 0, 0))
            self.child = subprocess.Popen(argv, env=env, cwd=cwd, stdin=slave, stdout=slave,
                                          stderr=slave, start_new_session=True)
            os.close(slave)
            reader = self.master
        else:
            self.child = subprocess.Popen(argv, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
                                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
            reader = self.child.stdout.fileno()

        def drain():
            pending = b""
            try:
                while True:
                    try:
                        data = os.read(reader, 65536)
                    except OSError as error:
                        if self.master is not None and error.errno == 5:
                            break
                        raise
                    if not data:
                        break
                    if self.master is not None:
                        if b"\x1b[6n" in data:
                            os.write(self.master, b"\x1b[1;1R")
                        if b"\x1b[c" in data:
                            os.write(self.master, b"\x1b[?1;2c")
                    pending += data
                    if len(pending) > BODY_CAP:
                        raise ValueError("client line cap")
                    # Keep the final fragment until the next read for complete redaction.
                    *lines, pending = pending.split(b"\n")
                    for line in lines:
                        self.log.write(line.decode("utf-8", "replace") + "\n")
                    self.tail = (self.tail + data.decode("utf-8", "replace"))[-100000:]
                if pending:
                    self.log.write(pending.decode("utf-8", "replace"))
            except Exception as error:
                self.failures.append(str(error))
                stop(self.child)

        self.thread = threading.Thread(target=drain, daemon=True)
        CHILDREN.add(self.child)
        self.thread.start()

    def text(self):
        return re.sub(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\x1b.", "", self.tail)

    def send(self, text):
        os.write(self.master, text.encode())
        time.sleep(0.15)
        os.write(self.master, b"\r")

    def close(self):
        stop(self.child)
        CHILDREN.discard(self.child)
        self.thread.join(timeout=5)
        if self.master is not None:
            os.close(self.master)


def disk_usage(roots):
    owned = 0
    files = []
    links = []
    for root in roots:
        for path in root.rglob("*"):
            try:
                info = path.lstat()
                owned += max(info.st_size, info.st_blocks * 512)
                if stat.S_ISREG(info.st_mode):
                    files.append((str(path.relative_to(root)), info.st_size))
                elif stat.S_ISLNK(info.st_mode):
                    links.append(dict(path=str(path.relative_to(root)), target=os.readlink(path),
                                      link_bytes=info.st_size))
            except FileNotFoundError:
                # Product state writes replace temporary files atomically.
                continue
    return dict(owned_bytes=owned, regular_files=files, symlinks=links)


def wait_for(predicate, deadline, processes, recorder=None, work=None):
    while not predicate():
        if time.monotonic() >= deadline:
            raise TimeoutError("run wall timeout")
        for process in processes:
            if process.failures:
                raise RuntimeError(process.failures[0])
            if process.child.poll() is not None:
                raise RuntimeError(f"process exited {process.child.returncode}")
        if recorder and recorder.failures:
            raise RuntimeError(recorder.failures[0])
        if work:
            usage = disk_usage([work])
            if usage["owned_bytes"] > SCRATCH_CAP:
                raise RuntimeError("scratch folder cap: 128 MiB")
            if any(size > LOG_CAP for path, size in usage["regular_files"]
                   if Path(path).suffix in [".log", ".jsonl", ".ndjson"]):
                raise RuntimeError("client-owned log cap")
            if recorder and disk_usage([work.parent, recorder.output.parent])["owned_bytes"] > DISK_CAP:
                raise RuntimeError("whole run disk cap: 3 GiB")
        time.sleep(0.05)


def main_requests(recorder, marker=None):
    wanted = [marker] if marker else ["SC239-MAIN", "SC239-SUBAGENT"]
    matched = []
    for request in recorder.requests:
        body = request["body"]
        if not body or not body.get("tools"):
            continue
        texts = []
        for message in body.get("messages", body.get("input", [])):
            if message.get("role") == "user":
                content = message["content"]
                texts.extend([content] if isinstance(content, str) else
                             [block["text"] for block in content if block["type"] in ["text", "input_text"]])
        if any(line.startswith(tag) for text in texts for line in text.splitlines() for tag in wanted):
            matched.append(request)
    return matched


def reply_completed(recorder, marker="SC239-MAIN"):
    return any(r.get("ended") and "SC239-OK" in r.get("response", "") for r in main_requests(recorder, marker))


def change_effort(api_client, deadline, processes, recorder=None, work=None):
    api_client.send("/effort high")
    wait_for(lambda: re.search(r"Change\s*effort\s*level\?", api_client.text()),
             deadline, processes, recorder, work)
    api_client.send("")
    wait_for(lambda: re.search(r"Set\s*effort\s*level\s*to\s*high", api_client.text()),
             deadline, processes, recorder, work)


def setup_client(check, work, port, claude, codex):
    config = work / "config"
    env = clean_env(work, f"http://127.0.0.1:{port + 1}")
    prompt = "SC239-MAIN Reply with SC239-OK only."
    if check.client == "claude":
        settings = dict(hasCompletedOnboarding=True, bypassPermissionsModeAccepted=True, theme="dark",
                        projects={str(work / "work"): dict(hasTrustDialogAccepted=True, hasCompletedProjectOnboarding=True)})
        if check.mode.startswith("advisor"):
            # Copy only this permitted leaf. Never copy account fields or the full config.
            source = Path.home() / ".claude.json"
            if source.stat().st_size > 8 * 1024 * 1024:
                raise RuntimeError("cached Claude flags source cap")
            flags = json.loads(source.read_text()).get("cachedGrowthBookFeatures")
            if not flags:
                raise RuntimeError("advisor cachedGrowthBookFeatures unavailable")
            settings["cachedGrowthBookFeatures"] = flags
        (config / ".claude.json").write_text(json.dumps(settings))
        (work / "mcp.json").write_text('{"mcpServers":{}}')
        env.update(CLAUDE_CONFIG_DIR=str(config), ANTHROPIC_BASE_URL=f"http://127.0.0.1:{port}",
                   ANTHROPIC_AUTH_TOKEN=KEY, ENABLE_TOOL_SEARCH="false")
        argv = [claude, "--model", MODEL, "--dangerously-skip-permissions", "--mcp-config", str(work / "mcp.json"),
                "--strict-mcp-config"]
        if check.mode.startswith("advisor"):
            argv += ["--advisor", MODEL]
        if check.mode == "advisor-experimental":
            env["CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL"] = "1"
        if check.mode == "effort":
            argv += ["--effort", "low"]
        if check.mode not in ["tui", "effort"]:
            argv += ["-p", prompt, "--verbose", "--output-format", "stream-json"]
    elif check.client == "codex":
        (config / "config.toml").write_text(f'''model = "{MODEL}"
model_provider = "sc239"
approval_policy = "never"
sandbox_mode = "danger-full-access"
cli_auth_credentials_store = "file"
check_for_update_on_startup = false
web_search = "disabled"

[model_providers.sc239]
name = "SC239 loopback"
base_url = "http://127.0.0.1:{port}/v1"
wire_api = "responses"
env_key = "SC239_KEY"

[analytics]
enabled = false

[projects."{work / 'work'}"]
trust_level = "trusted"
''')
        env.update(CODEX_HOME=str(config), SC239_KEY=KEY)
        # Keep default retry settings: the retry checks must measure the real client.
        argv = [codex, "exec", "--json", "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check", "-C", str(work / "work"), prompt]
    else:
        argv = [str(VENV / "bin/python"), "-B", str(HERE / "sdk-check.py"), str(port)]
    return argv, env


def dry_request(port, stream=True, effort=None, advisor=False, marker="SC239-MAIN", agent=False, continued=False, disconnect=False, header_disconnect=False):
    body = dict(model=MODEL, max_tokens=128, stream=stream,
                system=[{"type": "text", "text": "x-anthropic-billing-header: synthetic"},
                        {"type": "text", "text": "SC239 synthetic system prompt"}],
                messages=[{"role": "user", "content": marker}],
                tools=[{"name": "Agent" if agent else "probe", "input_schema": {"type": "object", "properties": {}}}])
    if effort:
        body["output_config"] = {"effort": effort}
    if continued:
        body["messages"] += [{"role": "assistant", "content": "SC239-OK"},
                             {"role": "user", "content": "SC239-MAIN second effort turn"}]
    if advisor:
        body["tools"].append({"type": "advisor_20260301", "name": "advisor", "model": MODEL})
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=40)
    conn.request("POST", "/v1/messages", json.dumps(body), {"x-api-key": KEY, "Content-Type": "application/json"})
    if header_disconnect:
        time.sleep(0.4)
        conn.close()
        return
    response = conn.getresponse()
    if disconnect:
        response.read(1)
        response.close()
        conn.close()
        return
    result = response.status, dict(response.getheaders()), response.read().decode()
    conn.close()
    return result


def acp_calls(host_text):
    calls = []
    for line in host_text.splitlines():
        if line.startswith("SC239 "):
            record = json.loads(line[6:])
            if record["kind"] == "hub" and record["event"]["kind"] == "got":
                calls.append(json.loads(record["event"]["line"]))
    return calls


def evaluate(check, recorder, host_text, client_text, exit_code, actions):
    calls = acp_calls(host_text)
    requests = main_requests(recorder)
    if check.scenario in ["temporary", "before", "after", "subagent", "nonstream"]:
        auxiliary = [r for r in recorder.requests if r not in requests]
        assert all("SC239 synthetic temporary upstream error" not in r.get("response", "") for r in auxiliary), "fault reached an auxiliary request"
    assert requests or check.client == "sdk", f"no main API request; client exit {exit_code}: {client_text.strip()[:300]}"
    if check.client == "sdk":
        sdk = [r for r in recorder.requests if r["path"] == "/v1/chat/completions"]
        assert len(sdk) == 2 and all(r.get("status") == 200 for r in sdk)
        assert sorted(bool(r["body"].get("stream")) for r in sdk) == [False, True]
        assert exit_code == 0 and '"stream": "PASS"' in client_text
        return dict(sdk_requests=2)
    if check.scenario in ["lockout", "sign-in"]:
        phrase = "lockout since" if check.scenario == "lockout" else "the official runtime is not signed in"
        assert len(requests) == 1, f"{len(requests)} main requests: unexpected retry"
        assert requests[0].get("status") == 400 and requests[0]["response_headers"].get("x-should-retry") == "false"
        assert phrase in client_text, "client did not show the state message"
        assert exit_code is not None and exit_code >= 0, "client did not stop normally"
    elif check.name.startswith("01"):
        first = requests[0]["body"]["system"][0]["text"]
        assert first.startswith("x-anthropic-billing-header:"), "attribution prefix changed"
        assert "x-anthropic-billing-header:" not in host_text, "attribution reached ACP"
        assert "SC239-OK" in client_text and exit_code == 0
        return dict(first_attribution_text=first, removed_from_acp=True)
    elif check.mode.startswith("advisor"):
        advertised = [r for r in requests if any(t.get("type", "").startswith("advisor_") for t in r["body"].get("tools", []))]
        assert advertised, "advisor was not advertised with the acp2api model name"
        reject = advertised[0]
        assert reject.get("status") == 400 and "Input tag 'advisor_" in reject.get("response", "")
        later = [r for r in requests if r["at"] > reject["at"]]
        assert later and all(not any(t.get("type", "").startswith("advisor_") for t in r["body"].get("tools", [])) for r in later)
        assert "SC239-OK" in client_text and exit_code == 0
    elif check.mode == "effort":
        efforts = [r["body"].get("output_config", {}).get("effort") for r in requests]
        assert "low" in efforts and "high" in efforts and actions == ["/effort high"], efforts
        levels = [call["params"]["value"] for call in calls if call["method"] == "session/set_config_option"
                  and call["params"]["configId"] == "thinking_level"]
        assert "low" in levels and "high" in levels, levels
        return dict(output_config_efforts=efforts, actions=actions)
    elif check.scenario == "pause":
        request = requests[0]
        frames = [e for e in recorder.events if e["id"] == request["id"]]
        keep = "ping" if check.client == "claude" else "response.in_progress"
        assert sum(e["event"] == keep for e in frames) >= 2, "keep-alive events absent"
        times = [request["at"]] + [e["at"] for e in frames] + [request["ended"]]
        assert max(b - a for a, b in zip(times, times[1:])) < 300
        assert request["ended"] - request["at"] >= 330
        assert "SC239-OK" in client_text and exit_code == 0
        return dict(duration=request["ended"] - request["at"], keep_alive_count=sum(e["event"] == keep for e in frames),
                    max_event_gap=max(b - a for a, b in zip(times, times[1:])))
    elif check.scenario == "nonstream":
        later = [r for r in requests if r["body"].get("stream") is False
                 and (r.get("status") == 200 or "client_disconnected_at" in r)]
        assert later, "Claude Code sent no accepted or disconnected non-stream request"
        long = later[0]
        if "client_disconnected_at" in long:
            duration = long["client_disconnected_at"] - long["at"]
            retries = [r["id"] for r in requests if r["at"] > long["at"]]
            raise AssertionError(f"original non-stream request {long['id']} disconnected after {duration:.3f}s; "
                                 f"client timeout header {long['headers'].get('x-stainless-timeout')}; replacement IDs {retries}")
        duration = long["ended"] - long["at"]
        assert duration > 300, f"non-stream request ended after {duration:.3f}s"
        assert len(later) == 1, "client replaced the original delayed non-stream request"
        assert "SC239-OK" in client_text and exit_code == 0, "client disconnected or retried before the delayed reply"
        return dict(nonstream_duration=duration)
    else:
        child = check.scenario == "subagent"
        measured = main_requests(recorder, "SC239-SUBAGENT") if child else requests
        if check.mode == "tui" and check.scenario == "after":
            assert "follow-up in retry wait" in actions, "no user follow-up during retry wait"
        errors = [e for e in recorder.events if e["event"] in ["error", "response.failed"]
                  and any(r["id"] == e["id"] for r in measured)]
        assert errors, "injected stream error absent"
        failed = errors[0]
        if check.scenario in ["after", "subagent"]:
            frames = [e for e in recorder.events if e["id"] == failed["id"] and e["at"] < failed["at"]]
            assert any(e["event"] == "content_block_delta" and "SC239-PARTIAL" in e.get("frame", "")
                       for e in frames), "invalid injection: error arrived before partial text"
            assert any(e["event"] == "content_block_stop" for e in frames), "invalid injection: partial text block was not closed"
        accepted = [r for r in measured if r["at"] > failed["at"] and r.get("status") == 200]
        assert accepted, "no successful retry after the error"
        gap = accepted[0]["at"] - failed["at"]
        assert gap >= 30, f"accepted retry too early: {gap:.3f}s"
        assert "SC239-OK" in client_text
        if check.client == "codex":
            assert all(r["at"] - failed["at"] >= 30 for r in measured if r["at"] > failed["at"]), "Codex retried before 30s"
        else:
            refused = [r for r in measured if failed["at"] < r["at"] < accepted[0]["at"]]
            assert refused and all(r.get("status") == 503 for r in refused), "retry wait did not refuse immediate requests"
            if check.scenario == "subagent" or (check.scenario == "after" and check.mode != "tui"):
                assert any("Your response above was cut off mid-stream" in json.dumps(r["body"]) for r in measured), "after-text retry omitted the resume message"
        return dict(retry_gap_seconds=gap, main_request_count=len(requests), actions=actions)
    return dict(main_request_count=len(requests))


def run_check(check, output, scratch, clients, dry=False):
    output.mkdir()
    work = scratch / check.name
    for child in ["home", "tmp", "config", "work"]:
        (work / child).mkdir(parents=True)
    upstream = local_port()
    recorder = Recorder(output, upstream, check.seconds)
    host = api_client = None
    actions = []
    result = dict(check=check.name, client=check.client, scenario=check.scenario, seconds_cap=check.seconds,
                  request_cap=REQUEST_CAP, log_cap_bytes=LOG_CAP, pause_seconds=check.pause,
                  status="FAIL", dry_run=dry)
    start = time.monotonic()
    deadline = start + check.seconds
    try:
        client_profile = work / "client.sb"
        host_profile = work / "host.sb"
        profile([recorder.port, recorder.proxy.server_port], client_profile)
        profile(["*"], host_profile)
        env = clean_env(work, f"http://127.0.0.1:{recorder.proxy.server_port}")
        argv = [str(TARGET / "debug/sc239-host"), str(work), str(upstream), check.scenario,
                str(check.seconds), str(check.pause), str(output.resolve())]
        host_argv = argv if dry else wrap(argv, env, host_profile)
        host = Process(host_argv, output / "host.log", env, work)
        wait_for(lambda: "management panel at http://127.0.0.1:" in host.tail,
                 min(deadline, time.monotonic() + 20), [host], recorder, work)
        if dry and check.client != "sdk":
            if check.mode.startswith("advisor"):
                rejected = dry_request(recorder.port, advisor=True)
                assert rejected[0] == 400 and "Input tag 'advisor_" in rejected[2]
            if check.mode == "effort":
                assert dry_request(recorder.port, effort="low")[0] == 200
                assert dry_request(recorder.port, effort="high", continued=True)[0] == 200
            elif check.name == "dry-disconnect":
                dry_request(recorder.port, disconnect=True)
                wait_for(lambda: any(call["method"] == "session/cancel"
                                     for call in acp_calls((output / "host.log").read_text())),
                         min(deadline, time.monotonic() + 3), [host], recorder, work)
                time.sleep(2.2)
                assert host.child.poll() is None and not host.failures
            else:
                first = dry_request(recorder.port, agent=check.scenario == "subagent")
                if check.scenario == "subagent":
                    assert '"tool_use"' in first[2] and '"Agent"' in first[2]
                    first = dry_request(recorder.port, marker="SC239-SUBAGENT")
                if check.scenario in ["lockout", "sign-in"]:
                    assert first[0] == 400 and first[1].get("x-should-retry") == "false"
                elif check.scenario in ["temporary", "before", "after", "nonstream", "subagent"]:
                    assert "SC239 synthetic temporary upstream error" in first[2]
                    if check.scenario in ["after", "subagent"]:
                        kinds = [e["event"] for e in recorder.events if e["id"] == recorder.requests[-1]["id"]]
                        assert "content_block_delta" in kinds, "after-text fault reached HTTP before any text delta"
                        assert kinds.index("content_block_delta") < kinds.index("content_block_stop") < kinds.index("error"), kinds
                        assert (output / "partial-seen.json").is_file(), "relay did not acknowledge forwarding partial text"
                    marker = "SC239-SUBAGENT" if check.scenario == "subagent" else "SC239-MAIN"
                    refusal = dry_request(recorder.port, stream=False, marker=marker)
                    assert refusal[0] == 503 and 1 <= int(refusal[1]["retry-after"]) <= 30
                    failed_at = next(e["at"] for e in recorder.events if e["event"] == "error")
                    wait_for(lambda: time.monotonic() - recorder.start >= failed_at + 30.3,
                             deadline, [host], recorder, work)
                    if check.name == "dry-nonstream-disconnect":
                        dry_request(recorder.port, stream=False, header_disconnect=True)
                        wait_for(lambda: any("client_disconnected_at" in r for r in recorder.requests),
                                 time.monotonic() + 3, [host], recorder, work)
                    retry_start = time.monotonic()
                    assert dry_request(recorder.port, stream=False, marker=marker)[0] == 200
                    if check.name == "dry-nonstream-disconnect":
                        assert time.monotonic() - retry_start < 1, "replacement request repeated the full pause"
                else:
                    assert first[0] == 200 and "SC239-OK" in first[2]
            calls = acp_calls((output / "host.log").read_text())
            if check.name == "dry-normal":
                appended = [call["params"]["_meta"]["antigravity"].get("systemPrompt", {}).get("append")
                            for call in calls if call["method"] == "session/new"]
                assert "SC239 synthetic system prompt" in appended
                assert "x-anthropic-billing-header:" not in json.dumps(calls)
            if check.mode == "effort":
                levels = [call["params"]["value"] for call in calls if call["method"] == "session/set_config_option"
                          and call["params"]["configId"] == "thinking_level"]
                assert levels == ["low", "high"], levels
            result.update(status="DRY-PASS", evidence="synthetic HTTP driver; real API clients pending")
        else:
            argv, client_env = setup_client(check, work, recorder.port, clients[0], clients[1])
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy"]:
                client_env[key] = env[key]
            sandboxed = argv if dry else wrap(argv, client_env, client_profile)
            tui = check.mode in ["tui", "effort"]
            api_client = Process(sandboxed, output / "client.log", client_env, work / "work", terminal=tui)
            if tui:
                wait_for(lambda: re.search(r"❯|shortcuts|bypass", api_client.text()),
                         min(deadline, time.monotonic() + 30), [host, api_client], recorder, work)
                time.sleep(0.8)
                api_client.send("SC239-MAIN Reply with SC239-OK only.")
                if check.mode == "effort":
                    wait_for(lambda: reply_completed(recorder), deadline, [host, api_client], recorder, work)
                    time.sleep(0.8)
                    change_effort(api_client, deadline, [host, api_client], recorder, work)
                    actions.append("/effort high")
                    before = len(main_requests(recorder))
                    api_client.send("SC239-MAIN second effort turn. Reply with SC239-OK only.")
                    wait_for(lambda: len(main_requests(recorder)) > before and
                             bool(main_requests(recorder)[-1].get("ended")), deadline, [host, api_client], recorder, work)
                elif check.scenario == "after":
                    wait_for(lambda: any(e["event"] == "error" for e in recorder.events),
                             deadline, [host, api_client], recorder, work)
                    time.sleep(2)
                    assert len([r for r in main_requests(recorder) if r.get("status") == 200]) == 1, "interactive main retried automatically"
                    api_client.send("SC239-MAIN follow-up in retry wait. Continue.")
                    actions.append("follow-up in retry wait")
                    wait_for(lambda: reply_completed(recorder), deadline, [host, api_client], recorder, work)
                else:
                    marker = "SC239-SUBAGENT" if check.scenario == "subagent" else "SC239-MAIN"
                    wait_for(lambda: reply_completed(recorder, marker), deadline, [host, api_client], recorder, work)
                time.sleep(1)
            else:
                wait_for(lambda: api_client.child.poll() is not None, deadline, [host], recorder, work)
                api_client.thread.join(timeout=2)
            if api_client.failures:
                raise RuntimeError(api_client.failures[0])
            details = evaluate(check, recorder, (output / "host.log").read_text(),
                               api_client.text() if tui else (output / "client.log").read_text(),
                               api_client.child.poll(), actions)
            result.update(status="DRY-PASS" if dry else "PASS", details=details)
    except Exception as error:
        failure = traceback.extract_tb(error.__traceback__)[-1]
        result.update(error=str(error) or f"{type(error).__name__} at {Path(failure.filename).name}:{failure.lineno}",
                      error_type=type(error).__name__, failure_line=f"{Path(failure.filename).name}:{failure.lineno}")
    finally:
        if api_client:
            result["client_exit"] = api_client.child.poll()
            api_client.close()
        if host:
            result["host_exit_before_cleanup"] = host.child.poll()
            (work / "stop").touch()
            try:
                host.child.wait(timeout=6)
            except subprocess.TimeoutExpired:
                pass
            result["host_graceful_exit"] = host.child.poll()
            host.close()
        recorder.close()
        result.update(seconds=round(time.monotonic() - start, 3), requests=recorder.count,
                      proxy_refusals=recorder.proxy_count, safety_failures=recorder.failures)
        if recorder.failures:
            result["status"] = "FAIL"
        save(output / "disk-inventory.json", disk_usage([work]))
        save(output / "result.json", result)
        save(output / "request-index.json", [{key: value for key, value in request.items() if key != "response"}
                                               for request in recorder.requests])
    print(f"{check.name}: {result['status']} ({result['seconds']}s) {redact(result.get('error', ''))}", flush=True)
    return result


def prepare(output, deadline):
    env = os.environ.copy()
    env.update(TMPDIR=str(BUILD / "tmp"), CARGO_TARGET_DIR=str(TARGET), CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", CARGO_INCREMENTAL="0", CARGO_BUILD_JOBS="4")
    result = command(["cargo", "build", "--workspace", "--locked", "--message-format=json"], output / "build.log",
                     min(900, deadline - time.monotonic()), env=env, cwd=PROJECT)
    assert result["exit_code"] == 0 and not result["stopped_by"], "workspace build failed"
    deps = TARGET / "debug/deps"
    argv = ["rustc", "--edition=2024", str(HERE / "research-host.rs"), "-L", f"dependency={deps}",
            "-o", str(TARGET / "debug/sc239-host")]
    artifacts = []
    for line in (output / "build.log").read_text().splitlines():
        if line.startswith("{"):
            value = json.loads(line)
            if value.get("reason") == "compiler-artifact":
                artifacts.append(value)
    for crate in ["acp2api", "acp2api_acceptance", "serde_json"]:
        matches = [Path(name).expanduser() for artifact in artifacts
                   if artifact["target"]["name"] == crate and "lib" in artifact["target"]["kind"]
                   for name in artifact["filenames"] if name.endswith(".rlib")]
        assert len(matches) == 1, f"expected one Cargo library artifact for {crate}"
        argv += ["--extern", f"{crate}={matches[0]}"]
    result = command(argv, output / "compile.log", 120, env=env, cwd=PROJECT)
    assert result["exit_code"] == 0 and not result["stopped_by"], "research host compile failed"
    if not VENV.exists():
        result = command([sys.executable, "-m", "venv", str(VENV)], output / "venv.log", 60)
        assert result["exit_code"] == 0 and not result["stopped_by"], "venv creation failed"
    result = command([str(VENV / "bin/python"), "-m", "pip", "install", "--disable-pip-version-check",
                      "-r", str(HERE / "requirements.txt")], output / "sdk-install.log", 180)
    assert result["exit_code"] == 0 and not result["stopped_by"], "official SDK installation failed"
    result = command([sys.executable, "-B", str(HERE / "self-check.py"), str(output / "lever-controls")],
                     output / "self-check.log", 20)
    assert result["exit_code"] == 0 and not result["stopped_by"], "lever safety controls failed"
    result = command([sys.executable, "-B", str(HERE / "round3-controls.py"), str(output / "round3-controls")],
                     output / "round3-controls.log", 20)
    assert result["exit_code"] == 0 and not result["stopped_by"], "round-3 lever controls failed"


def native_preflight(output, scratch, clients):
    for child in ["home", "tmp"]:
        (scratch / child).mkdir()
    env = clean_env(scratch, "http://127.0.0.1:9")
    for key in ["CODEX_HOME", "CLAUDE_CONFIG_DIR"]:
        folder = scratch / key.lower()
        folder.mkdir()
        env[key] = str(folder)
    sandbox = scratch / "preflight.sb"
    profile(["*"], sandbox)
    code = 'import socket; s=socket.socket(); s.settimeout(1); n=s.connect_ex(("192.0.2.1",9)); print(n); assert n in (1,13)'
    result = command(wrap([sys.executable, "-c", code], env, sandbox), output / "sandbox-control.log", 10)
    assert result["exit_code"] == 0 and not result["stopped_by"], "sandbox-exec unavailable or outbound control failed"
    for name, path in zip(["claude", "codex"], clients):
        assert path and Path(path).is_file(), f"{name} executable unavailable"
        result = command(wrap([path, "--version"], env, sandbox), output / f"{name}-version.log", 20)
        assert result["exit_code"] == 0 and not result["stopped_by"], f"{name} version check failed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=BUILD / "239-native" / time.strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("--dry-run", action="store_true", help="synthetic HTTP drivers; no Claude Code or Codex prompt")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--only", nargs="+", choices=[check.name for check in CHECKS], help="repeat the named native checks")
    selection.add_argument("--failed-from", type=Path, help="rerun only FAIL/BLOCKED names in a previous results.json")
    args = parser.parse_args()
    checks = [check for check in CHECKS if not args.only or check.name in args.only]
    if args.failed_from:
        try:
            previous = json.loads(args.failed_from.read_text())
            failed = {row["check"] for row in previous if row["status"] in ["FAIL", "BLOCKED"]}
            unknown = failed - {check.name for check in CHECKS}
            if not failed or unknown:
                parser.error(f"failed selection is empty or has unknown names: {sorted(unknown)}")
            checks = [check for check in CHECKS if check.name in failed]
        except (OSError, ValueError, KeyError, TypeError) as error:
            parser.error(f"cannot read previous results: {error}")
    if args.dry_run and (args.only or args.failed_from):
        parser.error("native selection cannot be combined with --dry-run")
    args.output.mkdir(parents=True, exist_ok=False)
    scratch_parent = BUILD / "tmp/239-client-checks"
    scratch_parent.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="sc239-", dir=scratch_parent))
    deadline = time.monotonic() + TOTAL_SECONDS
    # No shell EXIT trap: the Python finally blocks own every process group.
    claudes = list((Path.home() / "Library/Application Support/Claude/claude-code").glob("*/*/claude.app/Contents/MacOS/claude"))
    claudes.sort(key=lambda path: tuple(int(x) for x in path.parents[4].name.split(".")))
    clients = [str(claudes[-1]) if claudes else None, str(Path.home() / ".npm-global/bin/codex")]
    results = []
    def total_timeout():
        for process in list(CHILDREN):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        save(args.output / "total-timeout.json", dict(seconds_cap=TOTAL_SECONDS))
        for check in checks[len(results):]:
            print(f"{check.name}: BLOCKED whole run time cap", flush=True)
        os._exit(124)

    watchdog = threading.Timer(max(0, deadline - time.monotonic()), total_timeout)
    watchdog.daemon = True
    watchdog.start()
    signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
    try:
        prepare(args.output, deadline)
        save(args.output / "identity.json", dict(head=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=PROJECT).decode().strip(),
            clock="SystemClock", total_seconds_cap=TOTAL_SECONDS, scratch=str(scratch), dry_run=args.dry_run,
            selected_checks=[check.name for check in checks], failed_from=str(args.failed_from) if args.failed_from else None,
            scratch_cap_bytes=SCRATCH_CAP, whole_run_disk_cap_bytes=DISK_CAP,
            sha256={name: hashlib.sha256((TARGET / f"debug/{name}").read_bytes()).hexdigest()
                    for name in ["acp2api", "fake-acp-agent", "sc239-host"]},
            lever_sha256={path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in HERE.iterdir() if path.is_file()}))
        if not args.dry_run:
            native_preflight(args.output, scratch, clients)
        else:
            checks = [Check("dry-normal", "claude"), Check("dry-advisor", "claude", mode="advisor"),
                      Check("dry-effort", "claude", mode="effort"), Check("dry-pause", "claude", "pause", pause=2),
                      Check("dry-disconnect", "claude", "pause", pause=2),
                      Check("dry-lockout", "claude", "lockout"), Check("dry-sign-in", "claude", "sign-in"),
                      Check("dry-before", "claude", "before"), Check("dry-after", "claude", "after"),
                      Check("dry-nonstream", "claude", "nonstream", pause=2),
                      Check("dry-nonstream-disconnect", "claude", "nonstream", pause=4),
                      Check("dry-subagent", "claude", "subagent"), Check("09-openai-sdk", "sdk")]
        for check in checks:
            if time.monotonic() + check.seconds > deadline:
                raise TimeoutError("whole run time cap")
            results.append(run_check(check, args.output / check.name, scratch, clients, dry=args.dry_run))
    except (Exception, KeyboardInterrupt) as error:
        for check in checks[len(results):]:
            result = dict(check=check.name, status="BLOCKED", error=str(error))
            results.append(result)
            print(f"{check.name}: BLOCKED {redact(str(error))}", flush=True)
    save(args.output / "results.json", results)
    watchdog.cancel()
    print(f"Captures: {redact(str(args.output))}", flush=True)
    return int(any(result["status"] not in ["PASS", "DRY-PASS"] for result in results))


if __name__ == "__main__":
    sys.exit(main())
