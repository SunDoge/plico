#!/usr/bin/env python3
"""Move one DOM element across captured frames; record actual compositor layer diagnostics."""
import argparse
import asyncio
import base64
import io
import json
from pathlib import Path
import shutil
import subprocess
import time
import urllib.request

import cairo
import websockets

ROOT = Path(__file__).resolve().parent.parent


class CDP:
    def __init__(self, socket):
        self.socket, self.sequence, self.layers = socket, 0, []

    async def call(self, method, **params):
        self.sequence += 1
        current = self.sequence
        await self.socket.send(json.dumps(dict(id=current, method=method, params=params)))
        while True:
            message = json.loads(await asyncio.wait_for(self.socket.recv(), 15))
            if message.get('method') == 'LayerTree.layerTreeDidChange':
                self.layers = message['params'].get('layers', [])
            if message.get('id') == current:
                assert 'error' not in message, message
                return message.get('result', {})

    async def evaluate(self, expression):
        response = await self.call('Runtime.evaluate', expression=expression,
                                   awaitPromise=True, returnByValue=True)
        assert 'exceptionDetails' not in response, response
        return response['result'].get('value')


def pixels(encoded):
    raw = base64.b64decode(encoded)
    image = cairo.ImageSurface.create_from_png(io.BytesIO(raw))
    assert (image.get_width(), image.get_height()) == (120, 100)
    values = bytes(image.get_data())[2::4]
    mass = sum(values)
    assert mass > 0
    centroid = sum((index // 120)*value for index,value in enumerate(values))/mass
    return raw, values, centroid


async def verify_review(cdp, review, out):
    await cdp.call('Page.navigate', url=review.resolve().as_uri())
    assert await cdp.evaluate('''(async()=>{
      for(let i=0;i<100;i++){
        if(document.body?.dataset.state==='ready')return true;
        await new Promise(r=>setTimeout(r,20));
      }return false;
    })()''')
    results = []
    for dpr in (1,1.25,1.75):
        await cdp.call('Emulation.setDeviceMetricsOverride',width=1400,height=800,
                       deviceScaleFactor=dpr,mobile=False)
        for theme in ('dark','light'):
            for size in (12,14):
                for index in range(4):
                    expression = '''(async()=>{
                      byId('theme').value=THEME;byId('size').value=SIZE;
                      byId('scale').value='1.75';selected=INDEX;
                      await render();return {state:document.body.dataset.state,
                        metrics:JSON.parse(document.body.dataset.metrics)};
                    })()'''.replace('THEME',json.dumps(theme)).replace('SIZE',json.dumps(str(size))).replace('INDEX',str(index))
                    state = await cdp.evaluate(expression)
                    assert state['state']=='ready',state
                    m=state['metrics']
                    assert m['dpr']==dpr and m['x']==0 and m['y']==0,m
                    screenshot=await cdp.call('Page.captureScreenshot',format='png',fromSurface=True,
                        clip=dict(x=0,y=0,width=m['width']/dpr,height=m['height']/dpr,scale=1))
                    actual=cairo.ImageSurface.create_from_png(io.BytesIO(base64.b64decode(screenshot['data'])))
                    filename='coverage-preview.png' if index==3 else 'plico-text-quality-1.75x.png'
                    reference=cairo.ImageSurface.create_from_png(str(review.parent/theme/'1.75'/m['variant']/filename))
                    width,height=round(m['width']),round(m['height'])
                    assert (actual.get_width(),actual.get_height())==(width,height)
                    a,b=bytes(actual.get_data()),bytes(reference.get_data())
                    for y in range(height):
                        assert a[y*actual.get_stride():y*actual.get_stride()+width*4] == b[(y+round(m['sourceY']))*reference.get_stride():(y+round(m['sourceY']))*reference.get_stride()+width*4], (dpr,theme,size,index,y)
                    results.append(dict(dpr=dpr,theme=theme,size=size,variant=m['variant'],byte_identical=True))
    (out/'review-verification.json').write_text(json.dumps(results,indent=2))
    print(f'Review: {len(results)} source/screenshot pairs byte-identical',flush=True)


async def experiment(endpoint, out, review=None):
    async with websockets.connect(endpoint, max_size=16*1024*1024) as socket:
        cdp = CDP(socket)
        await cdp.call('Page.enable')
        await cdp.call('LayerTree.enable')
        await cdp.call('Emulation.setDeviceMetricsOverride', width=120, height=100,
                       deviceScaleFactor=1, mobile=False)
        if review is not None:
            await verify_review(cdp,review,out)
            return
        results = []
        for mode in ('layout', 'transform', 'layer'):
            folder = out / mode
            folder.mkdir(exist_ok=True)
            await cdp.call('Page.navigate', url='about:blank')
            setup = '''(async () => {
              document.body.innerHTML='';
              document.documentElement.style.background='black';
              document.body.style.margin='0';
              const font=new FontFace('Probe','local("NotoSans-Regular")');
              await font.load();document.fonts.add(font);await document.fonts.ready;
              const div=document.createElement('div');window.probe=div;
              Object.assign(div.style,{position:'absolute',left:'20px',top:'0px',
                color:'white',font:'14px Probe',lineHeight:'1',whiteSpace:'pre'});
              div.textContent='H';const marker=document.createElement('i');
              Object.assign(marker.style,{display:'inline-block',height:'0px',width:'0px',verticalAlign:'baseline'});
              div.append(marker);document.body.append(div);
              window.baseTop=32-marker.getBoundingClientRect().top;
              div.style.top=baseTop+'px';
              if (MODE==='layer') {div.style.willChange='transform';div.style.transform='translate3d(0,0,0)';}
              await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
              return document.fonts.check('14px Probe');
            })()'''.replace('MODE', json.dumps(mode))
            assert await cdp.evaluate(setup)
            records, previous = [], None
            for step in range(33):
                delta = step/32
                update = '''(async () => {
                  if (MODE==='layout') probe.style.top=(baseTop+DELTA)+'px';
                  else probe.style.transform=`translate3d(0,${DELTA}px,0)`;
                  await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
                  return probe.getBoundingClientRect().top;
                })()'''.replace('MODE',json.dumps(mode)).replace('DELTA',str(delta))
                top = await cdp.evaluate(update)
                screenshot = await cdp.call('Page.captureScreenshot', format='png', fromSurface=True)
                raw, values, centroid = pixels(screenshot['data'])
                (folder / f'{step:02}.png').write_bytes(raw)
                layers = []
                for layer in cdp.layers:
                    if not layer.get('drawsContent'):
                        continue
                    reasons = await cdp.call('LayerTree.compositingReasons', layerId=layer['layerId'])
                    layers.append(dict(id=layer['layerId'], paint_count=layer.get('paintCount'),
                                       width=layer['width'], height=layer['height'],
                                       reasons=reasons.get('compositingReasons', [])))
                records.append(dict(step=step, requested_delta=delta, actual_top=top,
                                    pixel_centroid=centroid,
                                    identical_to_previous=None if previous is None else previous==values,
                                    layers=layers))
                previous = values
            summary = dict(mode=mode, unchanged_steps=sum(r['identical_to_previous'] is True for r in records),
                           max_centroid_step=max(abs(b['pixel_centroid']-a['pixel_centroid'])
                                                 for a,b in zip(records,records[1:])),
                           total_motion=records[-1]['pixel_centroid']-records[0]['pixel_centroid'],
                           layer_diagnostics_available=any(r['layers'] for r in records))
            print(summary, flush=True)
            results.append(dict(summary=summary, frames=records))
        (out / 'measurements.json').write_text(json.dumps(results, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/chrome-live-text-motion')
    parser.add_argument('--chrome', default=shutil.which('google-chrome'))
    parser.add_argument('--review',type=Path,help='Verify the original-pixel A/B page instead of motion')
    args = parser.parse_args()
    assert args.chrome
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / 'environment.json').write_text(json.dumps(dict(
        chrome=subprocess.check_output([args.chrome, '--version'], text=True).strip(),
        scope='headless CDP same-element snapshots after two requestAnimationFrame callbacks; not frame pacing',
        requested_step=1/32, physical_scale=1), indent=2))
    # A fresh profile avoids reusing another browser's debugging endpoint.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='chrome-profile-', dir=out) as profile:
        log = (out / 'chrome.log').open('w')
        process = subprocess.Popen([args.chrome,'--headless=new','--remote-debugging-port=0',
            '--no-first-run','--no-default-browser-check','--disable-background-networking',
            f'--user-data-dir={profile}','about:blank'], stdout=log, stderr=log)
        try:
            port_file = Path(profile) / 'DevToolsActivePort'
            for _ in range(100):
                if port_file.exists():
                    break
                assert process.poll() is None, 'Chrome exited before CDP became available'
                time.sleep(.05)
            port = int(port_file.read_text().splitlines()[0])
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list',timeout=5) as response:
                pages = json.load(response)
            endpoint = next(page['webSocketDebuggerUrl'] for page in pages if page['type']=='page')
            asyncio.run(experiment(endpoint, out,args.review))
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            log.close()


if __name__ == '__main__':
    main()
