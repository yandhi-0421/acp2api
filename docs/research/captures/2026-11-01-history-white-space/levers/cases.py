"""Synthetic cases of the white space probe of issue 173. Each value is made up.

A step is one reply of the fake endpoint. A reply with a tool call makes the
client run the tool and ask again. A reply with no tool call ends a turn, and
the driver then sends the next user prompt.
"""
import json

PY_CODE = (
    "```python\n"
    "def A01(values):\n"
    "    total = 0  # two  spaces\n"
    "    for value in values:\n"
    "        if value:\n"
    "\t\ttotal += 1\n"
    "\n"
    "    return \"a  b\\tc\"   \n"
    "```\n"
)

TEXT = {
    "A01": PY_CODE,
    "A02": "A02 runs of spaces: a  b   c    d",
    "A03": "A03\ttabs\t\tinside",
    "A04": "   A04 leading spaces",
    "A05": "\n\n  A05 leading new lines",
    "A06": "A06 trailing spaces   ",
    "A07": "A07 final new line\n",
    "A08": "A08 final new lines\n\n\n",
    "A09": "A09 trailing mix \t \n  ",
    "A10": "A10 line one\r\nline two\r\n\r\nline four",
    "A11": "A11 final CRLF\r\n",
    "A12": "A12 lone\rcarriage return\r",
    "A13": "A13 nbsp x ideographic　y thin z zero-width​w",
    "A14": "A14 markdown hard break  \nnext line\n\n| a  |  b |\n|---|---|\n",
}

# One text delta for each list item. Some deltas hold only white space.
A15_CHUNKS = ["A15  split", "  ", "\n", "\n  in", "   deltas", "\n", "\n"]
# Two text blocks in one reply. acp2api emits one block, so this case is extra.
A16_BLOCKS = ["A16 first block\n", "  A16 second block  "]
# White space and no marker. The compare step finds it by position.
A17_ONLY_SPACE = "  \n\t  "
# White space at line ends and no white space at the ends of the text.
A19_INTERIOR = "A19 line one   \nline two\t\nline three"

RESULTS = {
    "R01": PY_CODE.replace("A01", "R01"),
    "R02": "   R02 leading and trailing spaces   ",
    "R03": "R03 final new lines\n\n\n",
    "R04": "R04 line one\r\n\tline two\t\r\n",
    "R05": "  \n\t  ",
    "R06": "",
    "R07": ["R07 first block\n", "  R07 second block  "],
    "R08": "  R08 error text  \n",
}

COMPACT = {
    "result_case": "R01",
    "text": "  C01 a  b\t\tc\r\nd\n\n  ",
    "note": "\n",
}

STEP_ARGS = {
    # The form of acp2api: compact JSON, in the order of the runtime.
    "T01": json.dumps(COMPACT, separators=(",", ":"), ensure_ascii=False),
    # JSON white space between tokens, and keys in a different order.
    "T02": '{\n  "note" : "  C02 note  " ,\n\t"text":   "C02 x  y\\n"  , "result_case" : "R02"\n}\n',
    # Number forms, keys that look like integers, and escapes in strings.
    "T03": '{"result_case":"R03","n":1.0,"m":1e2,"big":9007199254740993,'
           '"obj":{"b":1,"a":2,"10":3,"2":4},"text":"C03 \\u0041\\u00a0\\/x  \\t"}',
    "T04": json.dumps(
        {"result_case": "R04", "text": PY_CODE.replace("A01", "C04")},
        separators=(",", ":"),
        ensure_ascii=False,
    ),
    # Strings with only white space, and an empty string.
    "T05": '{"result_case":"R05","text":"C05 ok","note":"   \\n\\t","list":["","  "," x "]}',
    "T06": '{"result_case":"R06","text":"C06 final new line\\n"}',
    "T07": '{"result_case":"R07","text":"C07"}',
    "T08": '{"result_case":"R08","text":"C08"}',
}

STEP_TEXT = {
    "T01": "B01 text before a tool call\n\n",
    "T02": "   B02 spaces on both sides  \n",
    "T04": "B04 final space ",
    "T06": "\n\nB06 leading new lines",
}


def probe_tool_steps():
    steps = []
    for step_id, raw in STEP_ARGS.items():
        blocks = []
        if step_id in STEP_TEXT:
            blocks.append({"type": "text", "chunks": split(STEP_TEXT[step_id])})
        blocks.append({"type": "tool", "tool": "probe", "raw": raw})
        steps.append({"id": step_id, "blocks": blocks})
    return steps


def split(text):
    if len(text) < 4:
        return [text]
    third = len(text) // 3
    return [text[:third], text[third : 2 * third], text[2 * third :]]


def text_steps():
    steps = [
        {"id": case, "blocks": [{"type": "text", "chunks": split(value)}]}
        for case, value in TEXT.items()
    ]
    steps.append({"id": "A15", "blocks": [{"type": "text", "chunks": A15_CHUNKS}]})
    steps.append(
        {
            "id": "A16",
            "blocks": [{"type": "text", "chunks": [b]} for b in A16_BLOCKS],
        }
    )
    steps.append({"id": "A17", "blocks": [{"type": "text", "chunks": [A17_ONLY_SPACE]}]})
    # After A17, so that the prompt of A17 keeps its number in compare.py.
    steps.append({"id": "A19", "blocks": [{"type": "text", "chunks": split(A19_INTERIOR)}]})
    return steps


TURN_BREAK = {"id": "A20", "blocks": [{"type": "text", "chunks": ["A20 turn break"]}]}
AFTER_TOOLS = {"id": "A18", "blocks": [{"type": "text", "chunks": ["A18 after the tools  \n"]}]}
LAST = {"id": "Z99", "blocks": [{"type": "text", "chunks": ["Z99 end"]}]}

FILE_TEXT = "def D02():\n    x = 1   \n\t\ty = 2\t\r\n\r\n\n"
EDIT_OLD = "    x = 1"
EDIT_NEW = "    x = 3   \n\n  # D04 note   \t"


def claude_builtin_steps():
    """Keys in reverse schema order, and white space in the strings."""
    args = {
        "D01": ("Bash", {"description": "  D01 print  text  ", "command": "printf '  D01  a\\tb\\r\\nc   \\n\\n\\n'   "}),
        "D02": ("Write", {"content": FILE_TEXT, "file_path": "{WORK}/probe.py"}),
        "D03": ("Write", {"content": "D03 markdown  \n\n  text   \n\n", "file_path": "{WORK}/probe.md"}),
        "D04": ("Edit", {"new_string": EDIT_NEW, "old_string": EDIT_OLD, "file_path": "{WORK}/probe.py"}),
        "D05": ("Read", {"file_path": "{WORK}/probe.py"}),
    }
    return [
        {
            "id": step_id,
            "blocks": [
                {
                    "type": "tool",
                    "tool": tool,
                    "raw": json.dumps(value, separators=(",", ":"), ensure_ascii=False),
                }
            ],
        }
        for step_id, (tool, value) in args.items()
    ]


E_FILE = "def E01():\n    x = 1   \n\t\ty = 2\t\n\n"


def claude_schema_order_steps():
    """Keys in schema order, so that a change of a string shows alone."""
    calls = [
        ("E01", "Write", {"file_path": "{WORK}/e01.py", "content": E_FILE}),
        ("E02", "Write", {"file_path": "{WORK}/e02.py", "content": "def E02():\n    x = 1\n\n\n"}),
        ("E03", "Edit", {"file_path": "{WORK}/e01.py", "old_string": "    x = 1", "new_string": "    x = 3   \n  # E03 note  ", "replace_all": False}),
        ("E04", "Edit", {"file_path": "{WORK}/e02.py", "old_string": "    x = 1", "new_string": "    x = 4  # E04"}),
        ("E05", "Bash", {"command": "  printf E05  "}),
        ("E06", "Bash", {"command": "cd {WORK} && printf E06"}),
        ("E07", "Bash", {"command": "printf E07", "description": "E07 print"}),
        ("E08", "Read", {"file_path": "{WORK}/e02.py", "offset": 1, "limit": 2}),
        ("E09", "TaskCreate", {"subject": "  E09 subject  ", "description": "E09 text  \n\n"}),
    ]
    return [
        {"id": step_id, "blocks": [{"type": "tool", "tool": tool, "raw": json.dumps(value, separators=(",", ":"), ensure_ascii=False)}]}
        for step_id, tool, value in calls
    ]


PATCH_TEXT = (
    "*** Begin Patch\n"
    "*** Add File: patched.py\n"
    "+def D07():\n"
    "+    x = 1   \n"
    "+\t\ty = 2\t\n"
    "+\n"
    "*** End Patch\n"
)


def codex_builtin_steps():
    shell = {"cmd": "printf '  D02  a\\tb\\r\\nc   \\n\\n\\n'   "}
    return [
        {"id": "D02", "blocks": [{"type": "tool", "tool": "shell", "raw": json.dumps(shell, separators=(",", ":"))}]},
        {"id": "D07", "blocks": [{"type": "custom", "tool": "apply_patch", "input": PATCH_TEXT}]},
    ]


def script(client, skip=()):
    builtin = {
        "claude": claude_builtin_steps() + claude_schema_order_steps(),
        "codex": codex_builtin_steps(),
        "sillytavern": [],
    }[client]
    tools = probe_tool_steps()
    if client == "sillytavern":
        # ToolManager.RECURSE_LIMIT is 5, so a text turn splits the chain.
        tools.insert(4, TURN_BREAK)
    steps = text_steps() + tools + builtin + [AFTER_TOOLS, LAST]
    return [step for step in steps if step["id"] not in skip]


def prompts(steps):
    """One prompt for each turn. The turn ends at a step with no tool call."""
    count = sum(1 for step in steps if not has_tool(step))
    out = []
    for index in range(1, count + 1):
        pad = "  " if index % 4 == 0 else ""
        out.append(f"{pad}P{index:02d} go  on{pad}")
    return out


def has_tool(step):
    return any(block["type"] != "text" for block in step["blocks"])


def turn_ends(steps):
    """Step counts at which each turn ends."""
    return [index + 1 for index, step in enumerate(steps) if not has_tool(step)]
