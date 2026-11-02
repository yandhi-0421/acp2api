"""Build archives and check the release contract without a runtime installation."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import re
import signal
import socket
import stat
import subprocess
import tarfile
import tempfile
import threading
import time
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
TARGETS = {
    "aarch64-apple-darwin": ("Darwin", "arm64"),
    "x86_64-apple-darwin": ("Darwin", "x86_64"),
    "x86_64-pc-windows-msvc": ("Windows", "AMD64"),
    "aarch64-pc-windows-msvc": ("Windows", "ARM64"),
}


def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", value):
        raise ValueError("the workspace version must be safe for an archive name")
    if os.environ.get("GITHUB_EVENT_NAME") == "push":
        if os.environ.get("GITHUB_REF") != f"refs/tags/v{value}":
            raise ValueError("the tag must match v<workspace version>")
    return value


def executable(target):
    return "acp2api.exe" if "windows" in target else "acp2api"


def archive(target):
    suffix = ".zip" if "windows" in target else ".tar.gz"
    return DIST / f"acp2api-{version()}-{target}{suffix}"


def package(target):
    binary = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / target / "release" / executable(target)
    DIST.mkdir(exist_ok=True)
    path = archive(target)
    if path.suffix == ".zip":
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as output:
            output.write(binary, executable(target))
    else:
        with path.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as output:
                output.add(binary, arcname=executable(target), recursive=False)
    print(f"{path.name}: executable {binary.stat().st_size} bytes; archive {path.stat().st_size} bytes")


def unpack(target, folder):
    path = archive(target)
    name = executable(target)
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as source:
            if source.namelist() != [name] or not stat.S_ISREG(source.getinfo(name).external_attr >> 16):
                raise ValueError(f"{path.name}: expected one regular executable")
            data = source.read(name)
    else:
        with tarfile.open(path) as source:
            members = source.getmembers()
            if len(members) != 1 or members[0].name != name or not members[0].isfile():
                raise ValueError(f"{path.name}: expected one regular executable")
            if members[0].mode & 0o111 == 0:
                raise ValueError(f"{path.name}: executable permission is absent")
            data = source.extractfile(members[0]).read()
    binary = folder / name
    binary.write_bytes(data)
    binary.chmod(0o755)
    return binary


def start(target):
    expected_os, expected_cpu = TARGETS[target]
    if (platform.system(), platform.machine().lower()) != (expected_os, expected_cpu.lower()):
        raise ValueError("start the archive on its native platform")
    with tempfile.TemporaryDirectory(prefix="acp2api-archive-") as temporary:
        folder = Path(temporary)
        binary = unpack(target, folder)
        # This folder does not exist before acp2api starts.
        data = folder / "data"
        with socket.socket() as lease:
            lease.bind(("127.0.0.1", 0))
            port = lease.getsockname()[1]
        expected = f"acp2api: {version()}: management panel at http://127.0.0.1:{port}/"
        child = subprocess.Popen([str(binary), "--data-dir", str(data), "--port", str(port)],
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = queue.Queue()

        def read():
            count = 0
            while chunk := os.read(child.stdout.fileno(), 4096):
                count += len(chunk)
                if count > 64 * 1024:
                    output.put(None)
                    return
                output.put(chunk)

        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        captured = b""
        deadline = time.monotonic() + 60
        try:
            while time.monotonic() < deadline:
                try:
                    chunk = output.get(timeout=0.1)
                except queue.Empty:
                    if child.poll() is not None:
                        raise RuntimeError("the archive executable exited before the start line")
                    continue
                if chunk is None:
                    raise RuntimeError("start output exceeded 64 KiB")
                captured += chunk
                if expected in captured.decode(errors="replace").splitlines():
                    if child.poll() is not None:
                        raise RuntimeError("the archive executable exited after the start line")
                    # First-start credentials never enter the workflow log.
                    print(expected, flush=True)
                    return
            raise TimeoutError("no management panel address within 60 seconds")
        except (RuntimeError, TimeoutError) as error:
            lines = captured.decode(errors="replace").splitlines()
            safe = [line for line in lines if "local API key:" not in line and "management password:" not in line]
            terminal = "\n".join(safe[-10:]).replace(str(Path.home()), "~")
            raise RuntimeError(f"{error}; terminal: {terminal}") from None
        finally:
            if child.poll() is None:
                if os.name == "posix":
                    child.send_signal(signal.SIGINT)
                else:
                    child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            reader.join(timeout=2)
            child.stdout.close()


def checksums():
    expected = {archive(target).name for target in TARGETS}
    actual = {path.name for path in DIST.iterdir()} - {"SHA256SUMS"}
    if actual != expected:
        raise ValueError(f"expected four archives; missing={expected-actual}; extra={actual-expected}")
    with tempfile.TemporaryDirectory() as temporary:
        for target in TARGETS:
            unpack(target, Path(temporary))
    lines = [f"{hashlib.sha256((DIST / name).read_bytes()).hexdigest()}  {name}\n" for name in sorted(expected)]
    return "".join(lines)


def gh(*args):
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True, timeout=60).stdout


def draft():
    if os.environ.get("GITHUB_EVENT_NAME") != "push":
        raise ValueError("only a tag push can create a draft release")
    if (DIST / "SHA256SUMS").read_text() != checksums():
        raise ValueError("SHA256SUMS does not match the archives")
    tag = "v" + version()
    repo = os.environ["GITHUB_REPOSITORY"]
    # Listing drafts avoids treating an authentication or network error as a missing release.
    releases = json.loads(gh("api", "--paginate", "--slurp", f"repos/{repo}/releases"))
    existing = next((item for page in releases for item in page if item["tag_name"] == tag), None)
    if existing and not existing["draft"]:
        raise ValueError("refuse to change a published release")
    files = [str(path) for path in sorted(DIST.iterdir())]
    if existing:
        gh("release", "upload", tag, *files, "--clobber", "--repo", repo)
    else:
        gh("release", "create", tag, *files, "--draft", "--verify-tag", "--repo", repo,
           "--title", f"acp2api {version()}", "--notes", "The owner reviews and publishes this draft.")
    result = json.loads(gh("release", "view", tag, "--repo", repo, "--json", "isDraft,assets"))
    if not result["isDraft"] or {a["name"] for a in result["assets"]} != {Path(p).name for p in files}:
        raise ValueError("the release must remain a draft with exactly five assets")
    print(f"Draft {tag}: four archives and SHA256SUMS; the owner publishes it.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["version", "package", "start", "checksums", "draft"])
    parser.add_argument("target", nargs="?", choices=TARGETS)
    args = parser.parse_args()
    if args.command in {"package", "start"} and not args.target:
        parser.error("this command needs a target")
    if args.command == "version":
        print(version())
    elif args.command == "package":
        package(args.target)
    elif args.command == "start":
        start(args.target)
    elif args.command == "checksums":
        DIST.mkdir(exist_ok=True)
        (DIST / "SHA256SUMS").write_text(checksums(), encoding="utf-8", newline="\n")
        print("SHA256SUMS: four archives checked")
    else:
        draft()
