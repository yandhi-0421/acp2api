"""Run one client against the fake endpoint, offline, with limits.

  python3 run.py --client claude --mode print-stream --name cc-stream
  python3 run.py --client claude --mode print-resume --name cc-resume
  python3 run.py --client claude --mode tui --name cc-tui
  python3 run.py --client codex --mode exec-resume --name cx-resume
  python3 run.py --client codex --mode tui --name cx-tui
  python3 run.py --client sillytavern --mode headless --name st-stream

Each run has its own folder under PROBE_ROOT, with a new HOME and a new client
configuration. Raw request logs stay there. They do not go into the repository.
"""
import argparse
import fcntl
import json
import os
import pathlib
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import termios
import time
import urllib.error
import urllib.request

LEVERS = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(LEVERS))
sys.dont_write_bytecode = True
import cases

REAL_HOME = os.path.expanduser("~")
PROBE_ROOT = pathlib.Path(os.environ.get("PROBE_ROOT", f"{REAL_HOME}/acp2api-build/173-probe/runs"))
CLAUDE = os.environ.get(
    "PROBE_CLAUDE",
    f"{REAL_HOME}/Library/Application Support/Claude/claude-code/2.1.309/5af865b9901c/claude.app/Contents/MacOS/claude",
)
CODEX = os.environ.get("PROBE_CODEX", f"{REAL_HOME}/.npm-global/bin/codex")
NODE = os.environ.get("PROBE_NODE", f"{REAL_HOME}/.local/bin")
PYTHON = "/opt/homebrew/bin/python3"
ST_SOURCE = pathlib.Path(os.environ.get("PROBE_ST", f"{REAL_HOME}/Documents/GitHub/SillyTavern"))
ST_APP = pathlib.Path(os.environ.get("PROBE_ST_APP", f"{REAL_HOME}/acp2api-build/173-probe/st-app"))
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

parser = argparse.ArgumentParser()
parser.add_argument("--client", choices=["claude", "codex", "sillytavern"], required=True)
parser.add_argument("--mode", required=True)
parser.add_argument("--name", required=True)
parser.add_argument("--port", type=int, default=18731)
parser.add_argument("--max-seconds", type=int, default=240)
parser.add_argument("--max-requests", type=int, default=120)
parser.add_argument("--max-log-bytes", type=int, default=30_000_000)
parser.add_argument("--max-prompts", type=int, default=0)
parser.add_argument("--turn-seconds", type=int, default=45)
parser.add_argument("--model", default="gpt-5.5")
parser.add_argument("--skip", default="", help="comma list of step ids to leave out")
parser.add_argument("--no-stream", action="store_true", help="SillyTavern only")
args = parser.parse_args()

fake_port, proxy_port = args.port, args.port + 1
st_port, cdp_port = args.port + 2, args.port + 3
run = PROBE_ROOT / args.name
if run.exists():
    shutil.rmtree(run)
for sub in ("home", "work", "tmp", "config"):
    (run / sub).mkdir(parents=True)
run = run.resolve()
work = run / "work"
deadline = time.monotonic() + args.max_seconds
skip = [part for part in args.skip.split(",") if part]
steps = cases.script(args.client, skip)
prompts = cases.prompts(steps)
ends = cases.turn_ends(steps)
if args.max_prompts:
    prompts = prompts[: args.max_prompts]


def profile():
    deny = "".join(
        f'(deny file-read* file-write* (subpath "{REAL_HOME}/{path}"))'
        for path in (".codex", ".claude", "Library/Keychains")
    ) + f'(deny file-read* file-write* (literal "{REAL_HOME}/.claude.json"))'
    # The data and the secrets of the local SillyTavern stay out of reach.
    deny += f'(deny file-write* (subpath "{ST_SOURCE}"))(deny file-read* (subpath "{ST_SOURCE}/data"))(deny file-read* (literal "{ST_SOURCE}/config.yaml"))'
    ports = [fake_port, proxy_port]
    if args.client == "sillytavern":
        ports += [st_port, cdp_port]
    allow = "".join(f'(allow network-outbound (remote ip "localhost:{port}"))' for port in ports)
    return "(version 1)(allow default)(deny network-outbound)" + allow + deny


def env():
    proxy = f"http://127.0.0.1:{proxy_port}"
    base = {
        "HOME": str(run / "home"),
        "PATH": f"{NODE}:/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": str(run / "tmp"),
        "LANG": "en_US.UTF-8",
        "TERM": "xterm-256color",
        "USER": "probe",
        "LOGNAME": "probe",
        "SHELL": "/bin/zsh",
        "HTTPS_PROXY": proxy,
        "HTTP_PROXY": proxy,
        "https_proxy": proxy,
        "http_proxy": proxy,
        "NO_PROXY": "127.0.0.1,localhost",
        "no_proxy": "127.0.0.1,localhost",
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    if args.client == "claude":
        base.update(
            CLAUDE_CONFIG_DIR=str(run / "config"),
            ANTHROPIC_BASE_URL=f"http://127.0.0.1:{fake_port}",
            ANTHROPIC_AUTH_TOKEN="made-up-probe-token",
            ENABLE_TOOL_SEARCH="false",
        )
    elif args.client == "codex":
        base.update(CODEX_HOME=str(run / "config"), PROBE_FAKE_KEY="made-up-probe-token")
    return base


def wrap(argv):
    pairs = [f"{key}={value}" for key, value in env().items()]
    return ["/usr/bin/sandbox-exec", "-p", profile(), "/usr/bin/env", "-i", *pairs, *argv]


def claude_config():
    (run / "config" / ".claude.json").write_text(
        json.dumps(
            {
                "hasCompletedOnboarding": True,
                "bypassPermissionsModeAccepted": True,
                "theme": "dark",
                "projects": {str(work): {"hasTrustDialogAccepted": True, "hasCompletedProjectOnboarding": True}},
            }
        )
    )
    (run / "mcp.json").write_text(
        json.dumps({"mcpServers": {"probe": {"type": "stdio", "command": PYTHON, "args": [str(LEVERS / "mcp_echo.py")]}}})
    )


def codex_config():
    (run / "config" / "config.toml").write_text(
        f"""model = "{args.model}"
model_provider = "probe"
approval_policy = "never"
sandbox_mode = "danger-full-access"
cli_auth_credentials_store = "file"
check_for_update_on_startup = false
web_search = "disabled"

[model_providers.probe]
name = "probe"
base_url = "http://127.0.0.1:{fake_port}/v1"
wire_api = "responses"
env_key = "PROBE_FAKE_KEY"
stream_max_retries = 0
request_max_retries = 0

[analytics]
enabled = false

[projects."{work}"]
trust_level = "trusted"

[mcp_servers.probe]
command = "{PYTHON}"
args = ["{LEVERS / 'mcp_echo.py'}"]
"""
    )


def fake_state():
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(f"http://127.0.0.1:{fake_port}/__state", timeout=3) as reply:
        return json.load(reply)


def wait_ready():
    while time.monotonic() < deadline:
        try:
            return fake_state()
        except OSError:
            select.select([], [], [], 0.05)
    raise SystemExit("fake did not start")


def start_helpers():
    limit = str(args.max_seconds + 20)
    out = open(run / "helpers.log", "ab")
    fake = subprocess.Popen(
        [PYTHON, "-B", str(LEVERS / "fake.py"), "--port", str(fake_port), "--run-dir", str(run), "--client", args.client, "--work", str(work), "--max-requests", str(args.max_requests), "--max-seconds", limit, "--max-log-bytes", str(args.max_log_bytes), "--skip", args.skip],
        stdout=out, stderr=out, start_new_session=True,
    )
    proxy = subprocess.Popen(
        [PYTHON, "-B", str(LEVERS / "proxy.py"), "--port", str(proxy_port), "--log", str(run / "proxy.log"), "--max-seconds", limit],
        stdout=out, stderr=out, start_new_session=True,
    )
    return fake, proxy


def kill(process):
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    process.wait()


def remaining():
    return deadline - time.monotonic()


def wait_steps(target, poll=None):
    """Wait until the fake finished the step count, then until it is idle."""
    limit = min(deadline, time.monotonic() + args.turn_seconds)
    while time.monotonic() < limit:
        state = fake_state()
        if state["stopped"]:
            raise SystemExit(f"fake stopped: {state['stopped']}")
        if state["steps_done"] >= target and state["active"] == 0:
            return True
        if poll:
            poll()
        else:
            select.select([], [], [], 0.1)
    return False


def claude_common():
    return [
        CLAUDE,
        "--model", "claude-sonnet-4-5",
        "--dangerously-skip-permissions",
        "--mcp-config", str(run / "mcp.json"),
        "--strict-mcp-config",
    ]


def client_log(name):
    return open(run / name, "ab")


def claude_print_stream():
    argv = claude_common() + ["-p", "--verbose", "--input-format", "stream-json", "--output-format", "stream-json"]
    process = subprocess.Popen(wrap(argv), cwd=work, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=client_log("client.stderr"), start_new_session=True)
    done = 0
    try:
        for index, prompt in enumerate(prompts):
            line = {"type": "user", "message": {"role": "user", "content": prompt}}
            process.stdin.write((json.dumps(line) + "\n").encode())
            process.stdin.flush()
            if not read_result(process):
                print(f"no result after prompt {index + 1}")
                break
            done += 1
        return done
    finally:
        kill(process)


def read_result(process):
    limit = min(deadline, time.monotonic() + args.turn_seconds)
    size = 0
    with client_log("client.stdout") as log:
        while time.monotonic() < limit:
            ready, _, _ = select.select([process.stdout], [], [], 0.5)
            if not ready:
                continue
            line = process.stdout.readline()
            if not line:
                return False
            size += len(line)
            if size < 2_000_000:
                log.write(line)
            try:
                if json.loads(line).get("type") == "result":
                    return True
            except ValueError:
                pass
    return False


def claude_print_resume():
    session = None
    done = 0
    for index, prompt in enumerate(prompts):
        argv = claude_common() + ["-p", prompt, "--output-format", "json"]
        if session:
            argv += ["--resume", session]
        process = subprocess.Popen(wrap(argv), cwd=work, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=client_log("client.stderr"), start_new_session=True)
        try:
            out, _ = process.communicate(timeout=max(1, min(args.turn_seconds, remaining())))
        except subprocess.TimeoutExpired:
            kill(process)
            print(f"timeout at prompt {index + 1}")
            break
        with client_log("client.stdout") as log:
            log.write(out[:200_000] + b"\n")
        try:
            session = json.loads(out)["session_id"]
        except (ValueError, KeyError):
            print(f"no session id at prompt {index + 1}")
            break
        done += 1
    return done


def pty_spawn(argv):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 140, 0, 0))
    process = subprocess.Popen(wrap(argv), cwd=work, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    return process, master


class Screen:
    """Keep the pty drained. Keep a capped tail of the output."""

    def __init__(self, master):
        self.master = master
        self.tail = b""
        self.total = 0
        self.log = client_log("client.pty")

    def pump(self, seconds=0.1):
        ready, _, _ = select.select([self.master], [], [], seconds)
        if ready:
            try:
                data = os.read(self.master, 65536)
            except OSError:
                return False
            self.total += len(data)
            self.tail = (self.tail + data)[-20000:]
            if self.total < 3_000_000:
                self.log.write(data)
            self.answer_queries(data)
        return True

    def answer_queries(self, data):
        # A TUI asks for the cursor place and for device attributes.
        if b"\x1b[6n" in data:
            os.write(self.master, b"\x1b[1;1R")
        if b"\x1b[c" in data:
            os.write(self.master, b"\x1b[?1;2c")

    def wait(self, seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            self.pump(0.1)

    def text(self):
        plain = re.sub(rb"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07|\x1b.", b"", self.tail)
        return plain.decode("utf-8", "replace")


def pty_turns(argv, ready_marker, settle):
    process, master = pty_spawn(argv)
    screen = Screen(master)
    done = 0
    try:
        limit = min(deadline, time.monotonic() + 30)
        while time.monotonic() < limit:
            screen.pump(0.1)
            if re.search(ready_marker, screen.text()):
                break
        else:
            print("no input box on the screen")
            print(screen.text()[-1500:])
            return 0
        screen.wait(settle)
        for index, prompt in enumerate(prompts):
            os.write(master, prompt.strip().encode())
            screen.wait(0.4)
            os.write(master, b"\r")
            ok = wait_steps(ends[index], poll=lambda: screen.pump(0.1))
            if not ok:
                print(f"no reply after prompt {index + 1}")
                print(screen.text()[-1500:])
                break
            done += 1
            screen.wait(settle)
        return done
    finally:
        kill(process)
        os.close(master)


def claude_tui():
    return pty_turns(claude_common(), r"bypass|shortcuts|❯|>", 1.2)


def codex_common():
    return [CODEX, "--dangerously-bypass-approvals-and-sandbox", "--skip-git-repo-check"]


def codex_exec_resume():
    thread = None
    done = 0
    base = [CODEX, "exec", "--json", "--dangerously-bypass-approvals-and-sandbox", "--skip-git-repo-check", "-C", str(work)]
    for index, prompt in enumerate(prompts):
        if thread:
            argv = [CODEX, "exec", "resume", "--json", "--dangerously-bypass-approvals-and-sandbox", "--skip-git-repo-check", thread, prompt]
        else:
            argv = base + [prompt]
        process = subprocess.Popen(wrap(argv), cwd=work, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=client_log("client.stderr"), start_new_session=True)
        try:
            out, _ = process.communicate(timeout=max(1, min(args.turn_seconds, remaining())))
        except subprocess.TimeoutExpired:
            kill(process)
            print(f"timeout at prompt {index + 1}")
            break
        with client_log("client.stdout") as log:
            log.write(out[:200_000] + b"\n")
        for line in out.splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("type") == "thread.started":
                thread = event.get("thread_id")
        if not thread or process.returncode != 0:
            print(f"exit {process.returncode} at prompt {index + 1}, thread {bool(thread)}")
            break
        done += 1
    return done


def codex_tui():
    return pty_turns([CODEX, "--no-daemon", "--dangerously-bypass-approvals-and-sandbox", "-C", str(work)], r"Ask Codex|to send|context left|% left", 1.5)


def prepare_st():
    """Make a copy of the app with no data, no secrets, and no user extensions."""
    if ST_APP.exists():
        return
    ST_APP.mkdir(parents=True)
    for name in ("server.js", "src", "public", "default", "package.json", "webpack.config.js", "plugins.js", "index.d.ts", "jsconfig.json"):
        source = ST_SOURCE / name
        if source.exists():
            subprocess.run(["cp", "-cR", str(source), str(ST_APP / name)], check=True)
    (ST_APP / "plugins").mkdir()
    shutil.rmtree(ST_APP / "public/scripts/extensions/third-party", ignore_errors=True)
    (ST_APP / "public/scripts/extensions/third-party").mkdir()
    (ST_APP / "node_modules").symlink_to(ST_SOURCE / "node_modules")


def seed_st_settings():
    """Start with the Chat Completions API. Keep each power_user default."""
    settings = json.loads((ST_APP / "default/content/settings.json").read_text())
    settings["firstRun"] = False
    settings["main_api"] = "openai"
    settings["oai_settings"].update(
        chat_completion_source="custom",
        custom_url=f"http://127.0.0.1:{fake_port}/v1",
        custom_model="probe-model",
        stream_openai=not args.no_stream,
        function_calling=True,
    )
    folder = run / "st-data" / "default-user"
    folder.mkdir(parents=True)
    (folder / "settings.json").write_text(json.dumps(settings, indent=1))


def st_headless():
    prepare_st()
    seed_st_settings()
    server = subprocess.Popen(
        wrap([f"{NODE}/node", "server.js", "--port", str(st_port), "--dataRoot", str(run / "st-data"), "--configPath", str(run / "st-config.yaml"), "--listen", "false", "--browserLaunchEnabled", "false"]),
        cwd=ST_APP, stdin=subprocess.DEVNULL, stdout=client_log("st-server.log"), stderr=subprocess.STDOUT, start_new_session=True,
    )
    chrome = None
    try:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        limit = min(deadline, time.monotonic() + 240)
        while time.monotonic() < limit:
            try:
                opener.open(f"http://127.0.0.1:{st_port}/", timeout=3)
                break
            except urllib.error.HTTPError:
                break
            except OSError:
                if server.poll() is not None:
                    print("SillyTavern stopped")
                    return 0
                select.select([], [], [], 0.2)
        else:
            print("SillyTavern did not start")
            return 0
        chrome = subprocess.Popen(
            wrap([CHROME, "--headless=new", "--no-sandbox", f"--user-data-dir={run / 'chrome'}", f"--remote-debugging-port={cdp_port}", "--no-first-run", "--no-default-browser-check", "--disable-background-networking", "--disable-component-update", "--disable-sync", "--disable-extensions", "--use-mock-keychain", "--password-store=basic", f"--proxy-server=http://127.0.0.1:{proxy_port}", "--window-size=1280,900", "about:blank"]),
            stdin=subprocess.DEVNULL, stdout=client_log("chrome.log"), stderr=subprocess.STDOUT, start_new_session=True,
        )
        config = {
            "cdpPort": cdp_port, "appPort": st_port, "fakePort": fake_port, "prompts": prompts, "ends": ends,
            "results": cases.RESULTS, "deadlineSeconds": int(remaining()) - 5, "turnSeconds": args.turn_seconds,
            "stream": not args.no_stream, "outFile": str(run / "driver.json"),
        }
        (run / "driver-config.json").write_text(json.dumps(config))
        driver = subprocess.run(wrap([f"{NODE}/node", str(LEVERS / "st_driver.mjs"), str(run / "driver-config.json")]), capture_output=True, text=True, timeout=max(1, remaining()))
        (run / "driver.log").write_text((driver.stdout + driver.stderr)[-20000:])
        print(driver.stdout[-300:], driver.stderr[-300:])
        return json.loads((run / "driver.json").read_text())["done"] if (run / "driver.json").exists() else 0
    finally:
        if chrome:
            kill(chrome)
        kill(server)


MODES = {
    ("claude", "print-stream"): claude_print_stream,
    ("claude", "print-resume"): claude_print_resume,
    ("claude", "tui"): claude_tui,
    ("codex", "exec-resume"): codex_exec_resume,
    ("codex", "tui"): codex_tui,
    ("sillytavern", "headless"): st_headless,
}

if args.client == "claude":
    claude_config()
elif args.client == "codex":
    codex_config()
fake, proxy = start_helpers()
started = time.monotonic()
status = 1
try:
    wait_ready()
    done = MODES[(args.client, args.mode)]()
    state = fake_state()
    summary = {
        "name": args.name,
        "client": args.client,
        "mode": args.mode,
        "promptsSent": done,
        "promptsPlanned": len(prompts),
        "stepsDone": state["steps_done"],
        "stepsPlanned": len(steps),
        "requests": state["requests"],
        "mainRequests": state["main"],
        "logBytes": state["log_bytes"],
        "stopped": state["stopped"],
        "seconds": round(time.monotonic() - started, 1),
        "model": {"codex": args.model, "claude": "claude-sonnet-4-5", "sillytavern": "probe-model"}[args.client],
        "limits": {"maxRequests": args.max_requests, "maxSeconds": args.max_seconds, "maxLogBytes": args.max_log_bytes},
    }
    (run / "summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary))
    status = 0 if done == len(prompts) else 2
finally:
    kill(fake)
    kill(proxy)
sys.exit(status)
