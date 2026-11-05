#!/bin/sh
# Run outside the Codex sandbox. See README.md for bounds and captured evidence.
set -eu
levers=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 -B "$levers/runner.py" "$@"
