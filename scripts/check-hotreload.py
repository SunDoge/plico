#!/usr/bin/env python3
"""Exercise the actual CLI watcher, native dynamic loader and resident headless renderer.
Creates an isolated app; never edits the framework or the user's app sources.
"""
import json, os, queue, subprocess, tempfile, threading, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
TARGET=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
CLI=TARGET/'debug'/('plico.exe' if os.name=='nt' else 'plico')
REPORT=ROOT/'reports/hotreload';REPORT.mkdir(parents=True,exist_ok=True)
def texts(nodes):
    return [n['text'] for n in nodes]+[t for n in nodes for t in texts(n['children'])]
with tempfile.TemporaryDirectory(prefix='plico reload proof ') as temp:
    app=Path(temp)/'app'
    subprocess.run([str(CLI),'new',str(app),'--name','plico-reload-proof'],check=True)
    source=app/'src/lib.rs';original=source.read_text()
    log=(REPORT/'watcher.log').open('w')
    child=subprocess.Popen([str(CLI),'dev',str(app),'--headless'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1)
    output=queue.Queue()
    def reader():
        for line in child.stdout: output.put(line)
        output.put(None)
    threading.Thread(target=reader,daemon=True).start()
    def receive(timeout=120):
        line=output.get(timeout=timeout)
        if line is None: raise RuntimeError('CLI exited: '+(REPORT/'watcher.log').read_text()[-5000:])
        return json.loads(line)
    def request(action='snapshot',**kwargs):
        child.stdin.write(json.dumps(dict(action=action,**kwargs))+'\n');child.stdin.flush();return receive(30)
    def wait_for(predicate,timeout=90):
        until=time.monotonic()+timeout
        while time.monotonic()<until:
            value=request()
            if predicate(value): return value
            time.sleep(.15)
        raise AssertionError((REPORT/'watcher.log').read_text()[-5000:])
    try:
        print('Waiting for initial app/host build...',flush=True)
        ready=receive(300);assert ready['event']=='ready',ready;pid=ready['pid']
        first=request('click',role='button',label='Increment');assert first['state']['count']==1,first
        source.write_text('\n\n// whitespace-only movement of the component\n'+original.replace('Hello Plico','Reloaded Plico'))
        changed=wait_for(lambda v:'Reloaded Plico' in texts(v.get('nodes',[])))
        assert changed['pid']==pid and changed['state']['count']==1,changed
        def keys(nodes): return [(n['key'], keys(n['children'])) for n in nodes]
        assert keys(first['nodes'])==keys(changed['nodes']), 'generated IDs changed after text/whitespace edit'
        # Reading/compiling sources must not create an endless watch rebuild loop.
        before=(TARGET/'plico-dev/plico-reload-proof/build-report.json').read_text()
        time.sleep(.8)
        assert (TARGET/'plico-dev/plico-reload-proof/build-report.json').read_text()==before, 'idle watcher rebuilt without a source edit'
        build=json.loads((TARGET/'plico-dev/plico-reload-proof/build-report.json').read_text())
        rebuilt=[a for a in build['artifacts'] if not a['fresh']]
        assert rebuilt and all(a['target']=='plico_reload_proof' for a in rebuilt),rebuilt
        print('Hot edit preserved state/PID; dependencies were fresh.',flush=True)
        source.write_text(original+'\nTHIS WILL NOT COMPILE\n')
        until=time.monotonic()+90
        while 'build failed for plico-reload-proof' not in (REPORT/'watcher.log').read_text():
            assert time.monotonic()<until;time.sleep(.15)
        old=request('click',role='button',label='Increment');assert old['state']['count']==2 and old['pid']==pid,old
        schema=original.replace('count: u32','count: String').replace('self.count += 1;','self.count.push(\'!\');').replace('Hello Plico','Schema changed')
        source.write_text(schema)
        reset=wait_for(lambda v:'Schema changed' in texts(v.get('nodes',[])))
        assert reset['state']['count']=='' and reset['pid']==pid,reset
        source.write_text(schema.replace("fn show(&mut self, cx: &mut UiContext) {", "fn show(&mut self, cx: &mut UiContext) { panic!(\"candidate panic\");"))
        failed=wait_for(lambda v:'panicked' in str(v.get('error','')))
        assert failed['state']['count']=='' and failed['pid']==pid,failed
        mismatched=schema.replace('plico_dev::export_app!(Application);', '#[stabby::export] pub extern "C" fn plico_app_render(_input: stabby::vec::Vec<u8>) -> u32 { 0 }')
        source.write_text(mismatched)
        rejected=wait_for(lambda v:'ABI layout check' in str(v.get('error','')))
        assert rejected['pid']==pid and rejected['state']['count']=='',rejected
        source.write_text(schema.replace('Schema changed','Recovered Plico'))
        recovered=wait_for(lambda v:'Recovered Plico' in texts(v.get('nodes',[])) and v.get('error') is None)
        updated=request('click',role='button',label='Increment');assert updated['state']['count']=='!',updated
        request('capture',out=str(REPORT/'recovered.png'))
        result={'generated_keys_survive_text_and_whitespace_edit':True,'resident_pid_preserved':True,'compatible_state_preserved':True,'incompatible_state_reset':True,'compile_failure_kept_last_good':True,'panic_candidate_rejected':True,'abi_mismatch_rejected':True,'recovery_works':True,'only_app_rebuilt_after_edit':True,'idle_watcher_does_not_rebuild':True,'app_edit_build':build,'headless_native_capture':str(REPORT/'recovered.png')}
        (REPORT/'report.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps({k:v for k,v in result.items() if k!='app_edit_build'},indent=2),flush=True)
    finally:
        try:child.stdin.close();child.wait(timeout=15)
        except (OSError,subprocess.TimeoutExpired):child.kill();child.wait()
        log.close()
