#!/usr/bin/env python3
"""Verify modal state and exit deadlines through the real CLI/cdylib replay host."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
CLI = TARGET / 'debug' / ('plico.exe' if os.name == 'nt' else 'plico')
SOURCE = '''use plico::prelude::*;
static EFFECT: ShaderAsset = ShaderAsset::new("fn wabou_effect(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(wabou.values[0].x,0.25,uv.y,1.0); }");
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Application { dialog: ModalState, popup: PopoverState, selection: PopoverState, menu_state: PopoverState, channel: u8, actions: u64, count: u64, level: f64 }
impl plico_dev::App for Application {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        (column(cx).padding(20)).children(|cx| {
            if button(cx).text("Open dialog").clicked() { self.dialog.open(); }
            (modal(cx, &mut self.dialog).title("Preferences")).children(|cx| {
                text(cx, "Original content");
                slider(cx, "Volume", &mut self.level, 0.0..=10.0).step(0.5);
                shader(cx, &EFFECT).width(280.).height(80.).animated(true).values(&[self.count as f32, self.level as f32]);
                if button(cx).text("Increment").clicked() { self.count += 1; }
                text!(cx, "Count {}", self.count);
            });
            (popover(cx, &mut self.popup, "Quick options")).children(|cx| {
                text(cx, "Original popup");
                if button(cx).text("Increase and close").clicked() { self.count += 1; cx.close_popover(); }
            });
            let options = [SelectOption::new("stable", 0, "Stable"), SelectOption::new("nightly", 1, "Nightly")];
            select(cx, &mut self.selection, "Channel", &mut self.channel, &options);
            (menu(cx, &mut self.menu_state, "Commands")).children(|cx| {
                if menu_item(cx, "Duplicate").clicked() { self.actions += 1; }
            });
            (tooltip(cx, "Original tip")).children(|cx| { button(cx).text("Help"); });


        })
    }
}
plico_dev::export_app!(Application);
'''

def has_dialog(reply):
    return any(node['kind'] == 'Overlay' for node in reply.get('nodes', []))

def all_text(nodes):
    return [node['text'] for node in nodes] + [text for node in nodes for text in all_text(node['children'])]

with tempfile.TemporaryDirectory(prefix='plico-modal-reload-') as temporary:
    app = Path(temporary) / 'app'
    subprocess.run([str(CLI), 'new', str(app), '--name', 'plico-modal-proof'], check=True)
    source = app / 'src/lib.rs'
    source.write_text(SOURCE)
    with open('/tmp/plico-modal-reload-watcher.log', 'w') as log:
        child = subprocess.Popen([str(CLI), 'dev', str(app), '--headless'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, text=True, bufsize=1)
        messages = queue.Queue()
        def read():
            for line in child.stdout:
                messages.put(json.loads(line))
            messages.put(None)
        threading.Thread(target=read, daemon=True).start()
        def request(action='snapshot', **kwargs):
            child.stdin.write(json.dumps({'action': action, **kwargs}) + '\n')
            child.stdin.flush()
            reply = messages.get(timeout=30)
            if reply is None: raise RuntimeError('Development host exited; inspect /tmp/plico-modal-reload-watcher.log')
            return reply
        def wait_for(predicate):
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                reply = request()
                if predicate(reply):
                    return reply
                time.sleep(.1)
            raise AssertionError('Modal reload did not converge; inspect /tmp/plico-modal-reload-watcher.log')
        try:
            ready = messages.get(timeout=300)
            assert ready is not None and ready['event'] == 'ready', ready
            opened = request('click', role='button', label='Open dialog')
            assert has_dialog(opened), opened
            incremented = request('click', role='button', label='Increment')
            assert incremented['state']['count'] == 1, incremented
            numeric = request('set_value', label='Volume', value=7.3)
            assert numeric['state']['level'] == 7.5, numeric
            source.write_text(SOURCE.replace('Original content', 'Reloaded content').replace('vec4<f32>(wabou.values[0].x,0.25,uv.y,1.0)', 'vec4<f32>(0.25,wabou.values[0].x,uv.y,1.0)'))
            reloaded = wait_for(lambda reply: 'Reloaded content' in all_text(reply.get('nodes', [])))
            assert reloaded['pid'] == ready['pid'] and reloaded['state']['count'] == 1 and has_dialog(reloaded), reloaded
            def shaders(nodes):
                return [node for node in nodes if node.get('shader')] + [item for node in nodes for item in shaders(node['children'])]
            surfaces = shaders(reloaded['nodes'])
            assert len(surfaces) == 1 and surfaces[0]['shader']['values'] == [1.0, 7.5], surfaces
            assert 'vec4<f32>(0.25,wabou.values[0].x,uv.y,1.0)' in surfaces[0]['text'], surfaces
            capture = ROOT / 'reports/shaders/reload.png'
            capture.parent.mkdir(parents=True, exist_ok=True)
            request('capture', out=str(capture))
            closing = request('click', role='button', label='Close dialog')
            assert closing['state']['dialog']['open'] is False, closing
            closed = wait_for(lambda reply: not has_dialog(reply))
            assert closed['error'] is None, closed
            popup = request('click', role='button', label='Quick options')
            assert popup['state']['popup']['open'] is True, popup
            source.write_text(SOURCE.replace('Original content', 'Reloaded content').replace('Original popup', 'Reloaded popup'))
            popup_reload = wait_for(lambda reply: 'Reloaded popup' in all_text(reply.get('nodes', [])))
            assert popup_reload['pid'] == ready['pid'] and popup_reload['state']['popup']['open'] is True, popup_reload
            def flatten(nodes):
                return [node for parent in nodes for node in [parent, *flatten(parent['children'])]]
            popup_nodes = flatten(popup_reload['nodes'])
            anchors = [node['style']['anchor_id'] for node in popup_nodes if node['style'].get('anchor_id') is not None]
            targets = [node['style']['popup_anchor'] for node in popup_nodes if node['style'].get('popup_anchor') is not None]
            assert len(set(anchors)) == len(anchors) and len(targets) == 1 and targets[0] in anchors, (anchors, targets)
            chosen = request('click', role='button', label='Increase and close')
            assert chosen['state']['count'] == 2 and chosen['state']['popup']['open'] is False, chosen
            wait_for(lambda reply: not has_dialog(reply))
            request('click', role='combobox', label='Channel')
            source.write_text(SOURCE.replace('Nightly', 'Nightly build'))
            selected_reload = wait_for(lambda reply: 'Nightly build' in all_text(reply.get('nodes', [])))
            assert selected_reload['state']['selection']['open'] is True and selected_reload['state']['channel'] == 0, selected_reload
            select_capture = ROOT / 'reports/select-menu/select.png'
            select_capture.parent.mkdir(parents=True, exist_ok=True)
            request('capture', out=str(select_capture))
            selected = request('click', role='option', label='Nightly build')
            assert selected['state']['channel'] == 1 and selected['state']['selection']['open'] is False, selected
            wait_for(lambda reply: not has_dialog(reply))
            request('click', role='button', label='Commands')
            source.write_text(SOURCE.replace('Duplicate', 'Copy item'))
            menu_reload = wait_for(lambda reply: 'Copy item' in all_text(reply.get('nodes', [])))
            assert menu_reload['state']['menu_state']['open'] is True and menu_reload['state']['channel'] == 1, menu_reload
            request('capture', out=str(ROOT / 'reports/select-menu/menu.png'))
            activated = request('click', role='menuitem', label='Copy item')
            assert activated['state']['actions'] == 1 and activated['state']['menu_state']['open'] is False, activated
            wait_for(lambda reply: not has_dialog(reply))
            focused_tip = request('focus', role='button', label='Help')
            assert focused_tip['tooltip']['text'] == 'Original tip', focused_tip
            focus_id = focused_tip['tooltip']['focus']
            source.write_text(SOURCE.replace('Original tip', 'Reloaded tip'))
            tip_reload = wait_for(lambda reply: reply.get('tooltip') and reply['tooltip']['text'] == 'Reloaded tip')
            assert tip_reload['tooltip']['focus'] == focus_id and tip_reload['pid'] == ready['pid'], tip_reload
            tip_capture = ROOT / 'reports/tooltip/reload.png'
            tip_capture.parent.mkdir(parents=True, exist_ok=True)
            request('capture', out=str(tip_capture))
            report = {'tooltip_description_and_host_focus_survive_reload': True, 'select_and_menu_roles_state_and_events_replayed': True, 'popover_anchor_state_and_action_replayed': True, 'real_cdylib_reload': True, 'open_modal_and_state_preserved': True, 'resident_pid_preserved': True, 'close_event_replayed': True, 'exit_deadline_unmounts': True, 'animated_shader_payload_and_code_replayed': True, 'slider_numeric_event_and_state_replayed': True}
            destination = ROOT / 'reports/modal/reload.json'
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(json.dumps(report, indent=2) + '\n')
            print(json.dumps(report), flush=True)
        finally:
            child.stdin.close()
            try:
                child.wait(timeout=15)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
