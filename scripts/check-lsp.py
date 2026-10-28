#!/usr/bin/env python3
"""Probe real rust-analyzer completion/hover over stdio; no editor required."""
import json
import re
import pathlib
import queue
import subprocess
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
proc = subprocess.Popen(['rust-analyzer'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, cwd=ROOT)
messages = queue.Queue()

def reader():
    while True:
        headers = {}
        while line := proc.stdout.readline():
            if line == b'\r\n':
                break
            k, v = line.decode().split(':', 1)
            headers[k.lower()] = v.strip()
        if not headers:
            return
        messages.put(json.loads(proc.stdout.read(int(headers['content-length']))))

threading.Thread(target=reader, daemon=True).start()
serial = 0

def send(method, params=None, request=False):
    global serial
    msg = {'jsonrpc': '2.0', 'method': method, 'params': params}
    if request:
        serial += 1
        msg['id'] = serial
    raw = json.dumps(msg).encode()
    proc.stdin.write(f'Content-Length: {len(raw)}\r\n\r\n'.encode() + raw)
    proc.stdin.flush()
    return serial

def response(ident, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        msg = messages.get(timeout=max(.1, deadline - time.monotonic()))
        if msg.get('id') == ident and 'method' not in msg:
            if 'error' in msg:
                raise RuntimeError(msg['error'])
            return msg.get('result')
        if 'method' in msg and 'id' in msg:
            raw = json.dumps({'jsonrpc': '2.0', 'id': msg['id'], 'result': None}).encode()
            proc.stdin.write(f'Content-Length: {len(raw)}\r\n\r\n'.encode() + raw)
            proc.stdin.flush()
    raise TimeoutError(ident)

try:
    response(send('initialize',{'processId':None,'rootUri':ROOT.as_uri(),'capabilities':{'textDocument':{'completion':{'completionItem':{'snippetSupport':True}}}},'initializationOptions':{'checkOnSave':False,'procMacro':{'enable':True}},'workspaceFolders':[{'uri':ROOT.as_uri(),'name':ROOT.name}]},True))
    send('initialized')
    until=time.monotonic()+90
    while time.monotonic()<until:
        if response(send('workspace/symbol',{'query':'Ui'},True)):break
        time.sleep(.5)
    source=(ROOT/'crates/plico/examples/settings.rs').read_text()
    cases=[('component Ui','ui.text("Settings")','ui.tex','text'),('component builder','.font_size(if large { 28 } else { 20 })','.font_s','font_size'),('layout header','.padding(16)','.padd','padding'),('clock API','ui.text("Settings")','ui.no','now'),('frame request','ui.text("Settings")','ui.request_anim','request_animation_frame')]
    settings_path=ROOT/'crates/plico/examples/settings.rs'
    cases=[(label,settings_path,source,old,prefix,expected) for label,old,prefix,expected in cases]
    custom_path=ROOT/'crates/plico/examples/custom_widget.rs'
    custom=custom_path.read_text()
    cases += [('context-first button builder',custom_path,custom,'.primary()','.pri','primary'),
              ('context-first function from prelude',custom_path,custom,'button(cx).text("Save")','butt','button'),
              ('context-first base builder',custom_path,custom,'.padding(m.space(3))','.padd','padding'),
              ('context-first children body',custom_path,custom,'text(cx, label)','tex','text'),
              ('custom component function',custom_path,custom,'status_action(cx, "Custom component")','status_act','status_action'),
              ('UiContext methods',custom_path,custom,'cx.theme()','cx.the','theme'),
              ('formatted text builder',custom_path,custom,'.font_size(18)','.font_s','font_size'),
              ('switch disabled configuration',custom_path,custom,'.disabled(false)','.disa','disabled'),
              ('switch motion configuration',custom_path,custom,'.motion(Motion::Smooth)','.mot','motion'),
              ('checkbox response',custom_path,custom,'.response()','.res','response'),
              ('generated icon constructor',custom_path,custom,'icon(cx, &plico::icons::FOLDER)','ico','icon'),
              ('SVG icon color',custom_path,custom,'.color(Color::Muted)','.col','color'),
              ('full Lucide asset constant',custom_path,custom,'plico::icons::SEARCH','plico::icons::SEAR','SEARCH')]
    composed_path=ROOT/'crates/plico/tests/composite_widgets.rs'
    composed=composed_path.read_text()
    cases += [('composed disclosure function',composed_path,composed,'collapsible','collap','collapsible'),
              ('composed progress inside keyed closure',composed_path,composed,'progress(cx, "Download", value)','progr','progress')]
    input_path=ROOT/'crates/plico/tests/text_input.rs'
    input_source=input_path.read_text()
    cases += [('native text input placeholder',input_path,input_source,'.placeholder("Enter password")','.place','placeholder'),
              ('native text input response',input_path,input_source,'.response()','.res','response')]
    modal_path=ROOT/'apps/gallery/src/lib.rs'
    modal_source=modal_path.read_text()
    cases += [('modal title builder',modal_path,modal_source,'.title("Workspace preferences")','.tit','title'),
              ('modal close context method',modal_path,modal_source,'ui.close_modal()','ui.close_m','close_modal')]
    gallery_path=ROOT/'apps/gallery/src/lib.rs'
    gallery_source=gallery_path.read_text()
    cases += [('shader animated builder',gallery_path,gallery_source,'.animated(true)','.anim','animated'),
              ('shader uniforms builder',gallery_path,gallery_source,'.values(&[(0.8 + self.exposure * 0.7) as f32])','.val','values')]
    cases += [('slider step builder',gallery_path,gallery_source,'.step(0.05)','.ste','step')]
    cases += [('popover builder',gallery_path,gallery_source,'.close_popover()', '.close_pop', 'close_popover'),
              ('popover constructor',gallery_path,gallery_source,'plico::widgets::popover(ui, &mut self.quick_settings, "Quick settings")','plico::widgets::popo','popover')]
    cases += [('select width builder',gallery_path,gallery_source,'.width(132.)','.wid','width'),
              ('menu item icon builder',gallery_path,gallery_source,'.icon(&plico::icons::PLUS)','.ico','icon'),
              ('select constructor',gallery_path,gallery_source,'plico::widgets::select','plico::widgets::sel','select')]
    cases += [('tooltip constructor',gallery_path,gallery_source,'plico::widgets::tooltip','plico::widgets::tool','tooltip'),
              ('tooltip identity builder',gallery_path,gallery_source,'plico::widgets::tooltip(ui, "Add one to the counter")','plico::widgets::tooltip(ui, "Add one to the counter").ke','key')]
    button_path=ROOT/'crates/plico/tests/button_content.rs'
    button_source=button_path.read_text()
    cases += [('button text content',button_path,button_source,'.text("Save")','.te','text'),
              ('button icon content',button_path,button_source,'.icon(&icons::SAVE)','.ico','icon'),
              ('button accessible name',button_path,button_source,'.aria_label("Settings")','.aria','aria_label')]
    results=[]
    for label,path,source,old,prefix,expected in cases:
        # Formatting can insert whitespace between Rust tokens. Keep string tokens opaque.
        tokens=re.findall(r'"(?:\\.|[^"\\])*"|\w+|[^\s]',old)
        match=re.search(r'\s*'.join(re.escape(token) for token in tokens),source)
        assert match,(old,source)
        start,end=match.span();content=source[:start]+prefix+source[end:];offset=start+len(prefix)
        uri=path.as_uri()
        send('textDocument/didOpen',{'textDocument':{'uri':uri,'languageId':'rust','version':1,'text':content}})
        pos={'line':content[:offset].count('\n'),'character':len(content[:offset].rsplit('\n',1)[-1].encode('utf-16-le'))//2}
        result=response(send('textDocument/completion',{'textDocument':{'uri':uri},'position':pos},True))
        items=result.get('items',[]) if isinstance(result,dict) else result or []
        labels=sorted({i['label'] for i in items});found=any(x.split('(')[0]==expected for x in labels)
        results.append({'case':label,'expected':expected,'found':found,'labels':labels})
        send('textDocument/didClose',{'textDocument':{'uri':uri}})
    report={'server':subprocess.check_output(['rust-analyzer','--version'],text=True).strip(),'cases':results}
    path=ROOT/'reports/macros/lsp.json';path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2),flush=True)
    assert all(r['found'] for r in results),'completion regression'
    response(send('shutdown',request=True));send('exit')
finally:
    proc.terminate();proc.wait(timeout=5)
