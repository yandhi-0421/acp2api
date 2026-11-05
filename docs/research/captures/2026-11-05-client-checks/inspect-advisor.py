"""Inspect local advisor validation without launching Claude Code or sending HTTP."""
import argparse
import hashlib
import mmap
from pathlib import Path

from safety import save

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("executable", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
assert args.executable.stat().st_size <= 512 * 1024 * 1024, "executable size cap"
args.output.mkdir(parents=True, exist_ok=False)
with args.executable.open("rb") as file, mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as binary:
    # Native 2.1.309 includes readable source after its string pool.
    anchor = binary.find(b'function UFt(){if(i.CLAUDE_CODE_DISABLE_ADVISOR_TOOL')
    assert anchor >= 0, "advisor source module was not found"
    module = binary[anchor:anchor + 4000]
    functions = []
    for name in ["UFt", "xC", "Dc", "Ki", "Yq", "Pm", "Jge", "Uke"]:
        prefix = ("function " + name + "(").encode()
        start = module.index(prefix)
        depth = 0
        for end in range(module.index(b"{", start), len(module)):
            depth += (module[end] == ord("{")) - (module[end] == ord("}"))
            if depth == 0:
                end += 1
                break
        functions.append(dict(name=name, byte_offset=anchor + start,
                              source=module[start:end].decode()))
    guard = binary.find(b'if(t(`[AdvisorTool] --advisor ${s}`),!Yq(Fe))')
    assert guard >= 0, "startup main-model guard was not found"
    warning = binary.find(b"has no advisor rank in the model catalog. Switch to a public model alias", anchor)
    help_at = binary.find(b"Enable the server-side advisor tool with the specified model")
    help_end = binary.find(b"(alias or full ID).", help_at) + len(b"(alias or full ID).")
    save(args.output / "result.json", dict(
        executable=str(args.executable), sha256=hashlib.sha256(binary).hexdigest(),
        input_cap_bytes=512 * 1024 * 1024, http_request_cap=0, timeout_seconds=60,
        log_cap_bytes=4 * 1024 * 1024,
        help=dict(byte_offset=help_at, text=binary[help_at:help_end].decode()),
        functions=functions,
        startup_guard=dict(byte_offset=guard, source=binary[guard:guard + 260].decode()),
        warning=dict(byte_offset=warning, source=binary[warning:warning + 196].decode()),
        finding="The default startup main-model rank guard runs before advisor-value validation. Changing only the advisor value cannot bypass it.",
        experimental_route="The bundle explicitly names CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL=1. Dc makes Yq, Pm and Uke accept unranked models; Jge still checks model availability and consent.",
        native_experimental_acceptance="pending; no Claude Code process or model request ran in this inspection"))
print("advisor source inspection: experimental switch found; native acceptance pending")
