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
    path=ROOT/'crates/plico/examples/native_children.rs'
    source=path.read_text()
    cases=[(label,path,source,old,prefix,expected) for label,old,prefix,expected in [
        ('native layout header','.padding(20)','.padd','padding'),
        ('native children method','.children(|cx| {','.child','children'),
        ('nested builder','.primary()','.pri','primary'),
        ('context inside children','button(cx).text("Save")','cx.the','theme'),
        ('widget function','button(cx).text("Save")','butt','button'),
    ]]
    results=[]
    for label,path,source,old,prefix,expected in cases:
        assert old in source,(old,source)
        start=source.index(old);content=source[:start]+prefix+source[start+len(old):];offset=start+len(prefix)
        uri=path.as_uri()
        send('textDocument/didOpen',{'textDocument':{'uri':uri,'languageId':'rust','version':1,'text':content}})
        pos={'line':content[:offset].count('\n'),'character':len(content[:offset].rsplit('\n',1)[-1].encode('utf-16-le'))//2}
        result=response(send('textDocument/completion',{'textDocument':{'uri':uri},'position':pos},True))
        items=result.get('items',[]) if isinstance(result,dict) else result or []
        labels=sorted({i['label'] for i in items});found=any(x.split('(')[0]==expected for x in labels)
        results.append({'case':label,'expected':expected,'found':found,'labels':labels})
        send('textDocument/didClose',{'textDocument':{'uri':uri}})
    report={'server':subprocess.check_output(['rust-analyzer','--version'],text=True).strip(),'cases':results}
    path=ROOT/'reports/macros/native-children-lsp.json';path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2),flush=True)
    assert all(r['found'] for r in results), 'native children completion regression'
    response(send('shutdown',request=True));send('exit')
finally:
    proc.terminate();proc.wait(timeout=5)
