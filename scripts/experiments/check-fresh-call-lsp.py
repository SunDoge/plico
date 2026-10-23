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
    path=ROOT/'crates/plico-core/examples/operator_layout.rs'
    native=(ROOT/'crates/plico-core/examples/native_children.rs').read_text()
    shapes=[('native + component',native),
        ('native standalone',native.replace('#[component]\n','')),
        ('operator children',path.read_text())]
    snippets=[('new function prefix','te$'),('new function with semicolon','te$;'),
        ('open call, no closing paren','text($'),('auto-paired call, no semicolon','text($)'),
        ('auto-paired call, semicolon','text($);'),('context prefix, no semicolon','text(c$)'),
        ('context prefix, semicolon','text(c$);'),('second argument','text(cx, $);')]
    rows=[]
    for syntax,source in shapes:
        # Insert before an existing statement, as in the user's actual edit.
        anchors=[('root',source.index('text(cx,'))]
        if 'button(cx).text("Save")' in source:
            anchors.append(('nested',source.index('button(cx).text("Save")')))
        for location,anchor in anchors:
            for case,snippet in snippets:
                cursor=snippet.index('$')
                insertion=snippet.replace('$','')+'\n            '
                content=source[:anchor]+insertion+source[anchor:]
                offset=anchor+cursor
                uri=path.as_uri()
                send('textDocument/didOpen',{'textDocument':{'uri':uri,'languageId':'rust','version':1,'text':source}})
                send('textDocument/didChange',{'textDocument':{'uri':uri,'version':2},'contentChanges':[{'text':content}]})
                pos={'line':content[:offset].count('\n'),'character':len(content[:offset].rsplit('\n',1)[-1].encode('utf-16-le'))//2}
                result=response(send('textDocument/completion',{'textDocument':{'uri':uri},'position':pos,'context':{'triggerKind':1}},True))
                items=result.get('items',[]) if isinstance(result,dict) else result or []
                labels=sorted({i['label'] for i in items})
                sig=response(send('textDocument/signatureHelp',{'textDocument':{'uri':uri},'position':pos},True))
                signatures=sig.get('signatures',[]) if sig else []
                row={'syntax':syntax,'location':location,'case':case,'text_completion':any(x.split('(')[0]=='text' for x in labels),
                    'cx_completion':'cx' in labels,'signature':bool(signatures),'labels':labels,'signature_result':sig}
                rows.append(row)
                print(syntax+' / '+location+' / '+case+': text='+str(row['text_completion'])+' cx='+str(row['cx_completion'])+' signature='+str(row['signature']),flush=True)
                send('textDocument/didClose',{'textDocument':{'uri':uri}})
    report={'server':subprocess.check_output(['rust-analyzer','--version'],text=True).strip(),'cases':rows}
    out=ROOT/'reports/macros/fresh-call-lsp.json';out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(report,indent=2)+'\n')
    for row in rows:
        # Native children are recorded as a baseline: incomplete calls can show
        # the enclosing children signature. Require exact text signatures for the DSL.
        if row['syntax']=='operator children':
            if row['case'].startswith('new function'):
                assert row['text_completion'],row
            else:
                signatures=(row['signature_result'] or {}).get('signatures',[])
                assert any(signature['label'].startswith(('fn text','text(')) for signature in signatures),row
                if row['case']!='second argument':assert row['cx_completion'],row
    print('PASS: fresh-call recovery in native children and double-semicolon components',flush=True)
    response(send('shutdown',request=True));send('exit')
finally:
    proc.terminate();proc.wait(timeout=5)
