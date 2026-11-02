from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[1]
helpers = {
    "crates/acp2api/src/startup.rs": "duplicate_listener",
    "crates/acp2api-acceptance/src/client.rs": "duplicate_stream",
}
paths = subprocess.check_output(
    ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "crates"],
    cwd=root,
    text=True,
    timeout=10,
).split("\0")
seen = set()
violations = []
for path in paths:
    if not path.endswith(".rs"):
        continue
    source = (root / path).read_text(encoding="utf-8")
    if path in helpers:
        pattern = rf"(?m)^(?:pub )?fn {helpers[path]}\b[\s\S]*?^}}"
        matches = list(re.finditer(pattern, source))
        if len(matches) != 1:
            raise SystemExit(f"{path}: expected one safe socket duplicate helper")
        match = matches[0]
        source = source[:match.start()] + re.sub(r"[^\n]", " ", match[0]) + source[match.end():]
        seen.add(path)
    for match in re.finditer(r"\btry_clone\b", source):
        line = source.count("\n", 0, match.start()) + 1
        violations.append(f"{path}:{line}: use a safe socket duplicate helper")
if seen != helpers.keys():
    raise SystemExit("missing safe socket duplicate helper")
if violations:
    raise SystemExit("\n".join(violations))
print("Socket duplicate check passed")
