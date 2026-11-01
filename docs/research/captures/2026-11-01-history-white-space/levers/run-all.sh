#!/bin/zsh
# Run each client mode, offline, and write redacted captures.
#   zsh run-all.sh <capture-dir>
# Raw logs stay under PROBE_ROOT, default ~/acp2api-build/173-probe/runs.
# Each run has its own limits. See run.py and fake.py.
set -u
levers=${0:a:h}
capture=${1:a}
root=${PROBE_ROOT:-$HOME/acp2api-build/173-probe/runs}
export PROBE_ROOT=$root
limits=(--max-seconds 240 --max-requests 120 --max-log-bytes 40000000)
tools=T01,T02,T03,T04,T05,T06,T07,T08
one() {
  name=$1; shift
  python3 -B $levers/run.py --name $name "$@" $limits
  print "exit $name: $?"
  python3 -B $levers/extract.py $root/$name $capture
}
mkdir -p $capture
one cc-print-stream --client claude --mode print-stream --port 18801
one cc-print-resume --client claude --mode print-resume --port 18803
one cc-tui --client claude --mode tui --port 18805
one cx-exec-unknown-model --client codex --mode exec-resume --model acp2api-probe-model --skip D07 --port 18807
one cx-tui-unknown-model --client codex --mode tui --model acp2api-probe-model --skip D07 --port 18809
one cx-exec-gpt55 --client codex --mode exec-resume --model gpt-5.5 --skip $tools --port 18811
limits=(--max-seconds 300 --max-requests 120 --max-log-bytes 40000000)
one st-stream --client sillytavern --mode headless --port 18813
one st-no-stream --client sillytavern --mode headless --no-stream --port 18817
python3 -B $levers/compare.py $capture
