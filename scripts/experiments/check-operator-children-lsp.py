#!/usr/bin/env python3
"""Probe real rust-analyzer completion/hover over stdio; no editor required."""
import json
import pathlib
import queue
import subprocess
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
proc = subprocess.Popen(['rust-analyzer'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.DEVNULL, cwd=ROOT)
messages = queue.Queue()
published = {}

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
        if msg.get('method') == 'textDocument/publishDiagnostics':
            published[msg['params']['uri']] = msg['params'].get('diagnostics', [])
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
    init=response(send('initialize',{'processId':None,'rootUri':ROOT.as_uri(),'capabilities':{'textDocument':{'completion':{'completionItem':{'snippetSupport':True}}}},'initializationOptions':{'checkOnSave':False,'procMacro':{'enable':True}},'workspaceFolders':[{'uri':ROOT.as_uri(),'name':ROOT.name}]},True))
    send('initialized')
    until=time.monotonic()+90
    while time.monotonic()<until:
        if response(send('workspace/symbol',{'query':'UiContext'},True)): break
        time.sleep(.5)
    path=ROOT/'crates/plico/examples/operator_children.rs'
    source=path.read_text() + '\n#[component]\nfn renamed_probe(cx: &mut UiContext) { column(cx) + |inner| { text(inner, "Renamed").font_size(17); }; }\n'
    uri=path.as_uri(); version=0
    def open_text(content):
        global version
        version+=1
        send('textDocument/didOpen',{'textDocument':{'uri':uri,'languageId':'rust','version':version,'text':content}})
    def position(content, offset):
        return {'line':content[:offset].count('\n'),'character':len(content[:offset].rsplit('\n',1)[-1].encode('utf-16-le'))//2}
    results=[]
    cases=[
        ('layout chain','.padding(20)','.padd','padding'),
        ('widget function inside operator','text(cx, title)','tex','text'),
        ('nested widget function','text(cx, "Nested")','tex','text'),
        ('builder method','.primary()','.pri','primary'),
        ('context method inside operator','text(cx, title)','cx.the','theme'),
        ('context method inside nested operator','text(cx, "Nested")','cx.the','theme'),
        ('captured local inside operator','text(cx, title)','text(cx, tit','title'),
        ('mutable capture inside operator','*count += 1','*cou','count'),
        ('renamed context inside operator','text(inner, "Renamed")','inner.the','theme'),
        ('renamed context widget builder','.font_size(17)','.font_s','font_size'),
    ]
    # Run both under #[component] and as an ordinary Rust function.
    for macro in [True,False]:
        base=source if macro else source.replace('#[component]\n','')
        for label,old,prefix,expected in cases:
            start=base.index(old); content=base[:start]+prefix+base[start+len(old):]
            open_text(content)
            result=response(send('textDocument/completion',{'textDocument':{'uri':uri},'position':position(content,start+len(prefix))},True))
            items=result.get('items',[]) if isinstance(result,dict) else result or []
            labels=sorted({i['label'] for i in items})
            found=any(x.split('(')[0].split(':')[0].strip()==expected for x in labels)
            results.append({'case':label,'component':macro,'expected':expected,'found':found,'labels':labels})
            send('textDocument/didClose',{'textDocument':{'uri':uri}})
        open_text(base)
        offset=base.index('cx, "Nested"')
        hover=response(send('textDocument/hover',{'textDocument':{'uri':uri},'position':position(base,offset)},True))
        results.append({'case':'nested context hover','component':macro,'found':'UiContext' in json.dumps(hover),'response':hover})
        offset=base.index('text(cx, "Nested")')+1
        definition=response(send('textDocument/definition',{'textDocument':{'uri':uri},'position':position(base,offset)},True))
        results.append({'case':'nested text definition','component':macro,'found':bool(definition),'response':definition})
        # Insert a fresh text( call exactly as when typing; keep the suffix intact.
        send('textDocument/didClose',{'textDocument':{'uri':uri}})
        start=base.index('text(cx, "Nested")')
        content=base[:start]+'text('+base[start+len('text(cx, "Nested")'):]
        open_text(content)
        signature=response(send('textDocument/signatureHelp',{'textDocument':{'uri':uri},'position':position(content,start+5)},True))
        results.append({'case':'fresh text( signature help','component':macro,'found':bool(signature and signature.get('signatures')),'response':signature})
        send('textDocument/didClose',{'textDocument':{'uri':uri}})
    report={'server':subprocess.check_output(['rust-analyzer','--version'],text=True).strip(),'cases':results}
    output=ROOT/'reports/macros/operator-children-lsp.json';output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'server':report['server'],'cases':[{k:r[k] for k in ('case','component','found')} for r in results]},indent=2),flush=True)
    assert all(r['found'] for r in results), 'operator children LSP regression'
    response(send('shutdown',request=True));send('exit')
finally:
    proc.terminate();proc.wait(timeout=5)
