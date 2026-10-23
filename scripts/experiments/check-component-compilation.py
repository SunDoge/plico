#!/usr/bin/env python3
"""Check component compilation and preserve errors independently of LSP results."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[2]
build=subprocess.run(['cargo','build','-p','plico','--message-format=json','--quiet'],cwd=ROOT,capture_output=True,text=True,check=True)
artifacts=[json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
def library(name, suffix):
    return next(Path(file) for artifact in artifacts if artifact.get('reason')=='compiler-artifact' and artifact['target']['name']==name for file in artifact['filenames'] if file.endswith(suffix))
plico=library('plico','.rlib')
macros=next(Path(file) for artifact in artifacts if artifact.get('reason')=='compiler-artifact' and artifact['target']['name']=='plico_macros' for file in artifact['filenames'] if Path(file).suffix in ('.so','.dylib','.dll'))
deps=plico.parent if plico.parent.name=='deps' else plico.parent/'deps'
results=[]
with tempfile.TemporaryDirectory(prefix='plico-component-compile-') as directory:
    for name,path,expected in [
        ('retired syntax rejected',ROOT/'scripts/experiments/fixtures/retired_children.rs',False),
        ('native children',ROOT/'crates/plico-core/examples/native_children.rs',True),
        ('operator children',ROOT/'crates/plico-core/examples/operator_layout.rs',True),
        ('user unused locals remain diagnostics',ROOT/'scripts/experiments/fixtures/unused_component_local.rs',False),
        ('attribute recovery keeps error',ROOT/'scripts/experiments/fixtures/missing_semicolon_component.rs',False),
    ]:
        result=subprocess.run(['rustc','--edition=2024','--crate-type=lib','--crate-name=component_probe',str(path),
            '--extern',f'plico={plico}','--extern',f'plico_macros={macros}','-L',f'dependency={deps}',
            '-o',str(Path(directory)/'probe.rlib')],cwd=ROOT,capture_output=True,text=True)
        accepted=result.returncode==0
        results.append({'syntax':name,'compiles':accepted,'stderr':result.stderr})
        print(name+': '+('compiles' if accepted else 'rejected'))
        assert accepted==expected,result.stderr
        if name=='retired syntax rejected':assert 'layout;; children was removed' in result.stderr,result.stderr
        if name=='user unused locals remain diagnostics':assert 'unused variable: `unused_local`' in result.stderr,result.stderr
        if name=='attribute recovery keeps error':assert 'expected `;`' in result.stderr,result.stderr
out=ROOT/'reports/macros/component-compilation.json'
out.parent.mkdir(parents=True,exist_ok=True)
out.write_text(json.dumps(results,indent=2)+'\n')
