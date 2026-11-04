"""Rebuild the isolated measurement host and repeat the three bounded trials."""
import os
from pathlib import Path
import shutil
import subprocess

sources=Path(__file__).resolve().parent
root=Path(os.environ["ACP56_PROBE_OUTPUT"]).resolve()
root.mkdir(parents=True,exist_ok=True)
for name in ["measurement-host.rs","probe.rs","measure.py","cold_start.py"]:
    shutil.copy2(sources/name,root/name)
target=Path(os.environ['CARGO_TARGET_DIR'])

def run(*args, seconds=900):
    subprocess.run(args,check=True,timeout=seconds)

run('cargo','build','--workspace','--locked')
run('cargo','build','--release','--locked','-p','acp2api','--bin','acp2api','--target','aarch64-apple-darwin')
shutil.copy2(target/'debug/fake-acp-agent',root/'fake-acp-agent')
release=target/'aarch64-apple-darwin/release/deps'
debug=target/'debug/deps'
run('rustc','--edition=2024','-C','opt-level=3',str(root/'measurement-host.rs'),'-o',str(root/'measurement-host'),
    '-L',f'dependency={release}','-L',f'dependency={target / "release/deps"}',
    '--extern',f'acp2api={next(release.glob("libacp2api-*.rlib"))}',seconds=60)
run('rustc','--edition=2024',str(root/'probe.rs'),'-o',str(root/'probe'),
    '-L',f'dependency={debug}','--extern',f'acp2api_acceptance={target / "debug/libacp2api_acceptance.rlib"}',
    '--extern',f'serde_json={next(debug.glob("libserde_json-*.rlib"))}',seconds=60)
for trial in range(1,4):
    run('python3',str(root/'measure.py'),f'memory-{trial}.json',seconds=90)
