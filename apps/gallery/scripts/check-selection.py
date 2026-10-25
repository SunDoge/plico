"""After cargo build -p gallery: xvfb-run -a python3 apps/gallery/scripts/check-selection.py.
Requires xdotool, xclip; set PLICO_SELECTION_CAPTURE for an optional screenshot.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
def xdo(*args):
    return subprocess.run(["xdotool", *map(str,args)],check=True,capture_output=True,text=True).stdout.strip()
def clipboard():
    result=subprocess.run(["xclip","-selection","clipboard","-o"],capture_output=True,text=True,timeout=3)
    return result.stdout if result.returncode==0 else ""
def wait_for(check,seconds=20):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        value=check()
        if value:return value
        time.sleep(.05)
    raise AssertionError("selection smoke timeout")
with tempfile.TemporaryDirectory(prefix="plico-selection-") as folder:
    with (Path(folder)/"app.log").open("w") as log:
        app=subprocess.Popen([str(ROOT/"target/debug/gallery")],cwd=ROOT,
            env=dict(os.environ,WINIT_UNIX_BACKEND="x11",WAYLAND_DISPLAY=""),stdout=log,stderr=log)
        try:
            def find():
                result=subprocess.run(["xdotool","search","--onlyvisible","--name","^Plico gallery$"],capture_output=True,text=True)
                return result.stdout.strip().splitlines()[0] if result.stdout.strip() else None
            window=wait_for(find)
            xdo("windowfocus","--sync",window)
            time.sleep(1)
            xdo("mousemove","--window",window,85,351,"click",1)
            time.sleep(.5)
            xdo("mousemove","--window",window,450,182,"click",1)
            time.sleep(2)
            xdo("mousemove","--window",window,600,500,"click","--repeat",15,"--delay",30,4)
            time.sleep(.5)
            xdo("mousemove","--window",window,215,490,"mousedown",1)
            xdo("mousemove","--window",window,580,510,"mouseup",1)
            xdo("key","--clearmodifiers","ctrl+a")
            xdo("key","--clearmodifiers","ctrl+c")
            text=wait_for(clipboard)
            assert "A small interface, in motion" in text,repr(text)
            assert "You can replay this answer" in text,repr(text)
            assert "**calm" not in text
            assert "What makes a streaming interface" not in text
            if capture:=os.environ.get("PLICO_SELECTION_CAPTURE"):
                subprocess.run(["import","-window",window,capture],check=True)
            print(json.dumps({"clipboard_bytes":len(text.encode()),"cross_paragraph_copy":True,"markdown_syntax_stripped":True}))
        except Exception:
            print((Path(folder)/"app.log").read_text())
            raise
        finally:
            app.terminate()
            try:app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                app.kill();app.wait()
