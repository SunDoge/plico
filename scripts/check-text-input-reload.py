#!/usr/bin/env python3
"""Verify native input through a real resident host and replaceable cdylib."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time
ROOT=Path(__file__).resolve().parents[1]
TARGET=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target')).resolve()
CLI=TARGET/'debug/plico'
SOURCE='''use plico::prelude::*;
#[derive(Default,serde::Serialize,serde::Deserialize)]
#[serde(default)]
pub struct Application { #[serde(skip)] input:TextBuffer, saved:String, submits:u64 }
impl plico_dev::App for Application {
    #[component]
    fn show(&mut self,cx:&mut UiContext) {
        (column(cx).padding(20)).children(|cx| {
            text(cx,"Original form");
            if text_input(cx,"Name",&self.input).placeholder("Your name").response().submitted { self.submits+=1; }
            if button(cx).text("Save").clicked() {self.saved=self.input.text().unwrap();}
            if button(cx).text("Clear").clicked() {self.input.clear();}
        })
    }
}
plico_dev::export_app!(Application);
'''
with tempfile.TemporaryDirectory(prefix='plico-input-reload-') as temporary:
    app=Path(temporary)/'app'
    subprocess.run([str(CLI),'new',str(app),'--name','plico-input-proof'],check=True)
    source=app/'src/lib.rs';source.write_text(SOURCE)
    with open('/tmp/plico-input-reload-watcher.log','w') as log:
        child=subprocess.Popen([str(CLI),'dev',str(app),'--headless'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1)
        messages=queue.Queue()
        def read():
            for line in child.stdout: messages.put(json.loads(line))
            messages.put(None)
        threading.Thread(target=read,daemon=True).start()
        def request(action='snapshot',**kwargs):
            child.stdin.write(json.dumps(dict(action=action,**kwargs))+'\n');child.stdin.flush()
            reply=messages.get(timeout=30)
            assert reply is not None,Path('/tmp/plico-input-reload-watcher.log').read_text()[-4000:]
            assert reply.get('error') is None,reply
            return reply
        try:
            ready=messages.get(timeout=300);assert ready['event']=='ready',ready
            # No resize/animation/click warm-up: binding must exist on the first frame.
            request('focus',role='textbox',label='Name')
            request('text',text='hello 中文')
            saved=request('click',role='button',label='Save');assert saved['state']['saved']=='hello 中文',saved
            request('focus',role='textbox',label='Name')
            submitted=request('key',key='Enter');assert submitted['state']['submits']==1,submitted
            assert request()['state']['submits']==1
            source.write_text(SOURCE.replace('Original form','Reloaded form'))
            deadline=time.monotonic()+120
            while True:
                reply=request()
                if reply['reloads']>0:break
                assert time.monotonic()<deadline,'reload timed out'
                time.sleep(.15)
            after=request('click',role='button',label='Save')
            assert after['state']['saved']=='hello 中文' and after['state']['submits']==1,after
            request('focus',role='textbox',label='Name')
            assert request('key',key='Enter')['state']['submits']==2
            request('click',role='button',label='Clear')
            assert request('click',role='button',label='Save')['state']['saved']==''
            assert after['pid']==ready['pid']
            print('PASS: first-frame binding, native UTF-8 edit, save, submit, reload without duplicate submit, module clear')
        finally:
            try:child.stdin.close();child.wait(timeout=15)
            except (OSError,subprocess.TimeoutExpired):child.kill();child.wait()
