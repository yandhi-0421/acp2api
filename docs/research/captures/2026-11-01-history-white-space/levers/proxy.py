"""Local proxy that logs the target host of each request and refuses it.

Limits: --max-seconds, --max-lines. It logs no body and no header.
"""
import argparse
import os
import socketserver
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument("--port", type=int, required=True)
parser.add_argument("--log", required=True)
parser.add_argument("--max-seconds", type=int, default=300)
parser.add_argument("--max-lines", type=int, default=2000)
args = parser.parse_args()
lock = threading.Lock()
lines = 0


def lifetime():
    time.sleep(args.max_seconds)
    os._exit(3)


threading.Thread(target=lifetime, daemon=True).start()


class Handler(socketserver.StreamRequestHandler):
    timeout = 10

    def handle(self):
        global lines
        try:
            first = self.rfile.readline(2048).decode("latin-1").strip()
        except OSError:
            return
        parts = first.split()
        verb = parts[0] if parts else "?"
        target = parts[1] if len(parts) > 1 else "?"
        host = target.split("/")[2] if "://" in target else target
        with lock:
            lines += 1
            if lines <= args.max_lines:
                with open(args.log, "a", encoding="utf-8") as log:
                    log.write(f"{verb} {host}\n")
        try:
            self.wfile.write(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        except OSError:
            pass


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


Server(("127.0.0.1", args.port), Handler).serve_forever()
