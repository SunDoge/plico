// npm install --prefix /tmp/plico-web-host-qa playwright
// NODE_PATH=/tmp/plico-web-host-qa/node_modules node crates/plico-web-host/web/smoke.cjs
const assert = require("node:assert/strict");
const { chromium } = require("playwright");
(async () => {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? {executablePath:process.env.CHROMIUM_PATH} : {}),
    args: ["--no-sandbox", "--enable-unsafe-swiftshader", "--use-angle=swiftshader"],
  });
  try {
    for (const scale of [1, 2]) {
      const context = await browser.newContext({viewport:{width:1120,height:820},deviceScaleFactor:scale});
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", e => errors.push(String(e)));
      await page.goto(process.env.PLICO_WEB_URL || "http://127.0.0.1:8080");
      await page.waitForFunction(() => window.plico?.state || !document.querySelector("#error").hidden);
      async function healthy() {
        assert.equal(await page.locator("#error").textContent(), "");
        assert.deepEqual(errors, []);
      }
      async function click(label) {
        const node = await page.evaluate(label => window.plico.semantics().find(n => n.label === label && !n.disabled), label);
        assert.ok(node, label);
        const [x0,y0,x1,y1] = node.bounds;
        await page.mouse.click((x0+x1)/2, (y0+y1)/2);
      }
      await healthy();
      await click("Controls");
      await page.waitForFunction(() => window.plico.state.router.current === "Controls");
      for (let i=0;i<3;i++) await click("Increment");
      await page.waitForFunction(() => window.plico.state.count === 3);
      await click("Dark theme");
      await page.waitForFunction(() => window.plico.state.dark);
      await click("Data table");
      await page.waitForFunction(() => window.plico.state.router.current === "DataTable");
      await click("Filter projects");
      const inputValue = value => page.waitForFunction(value =>
        window.plico.semantics().some(n=>n.label==="Filter projects" && n.value===value), value);
      await page.waitForFunction(()=>document.activeElement?.matches('input[aria-label="Text input"]'));
      const initialTop = await page.evaluate(() => {
        const top = document.activeElement.getBoundingClientRect().top;
        const spacer = document.createElement("div");
        spacer.id = "scroll-fixture";
        spacer.style.height = "200px";
        document.body.append(spacer);
        document.body.scrollTop = 60;
        return top;
      });
      await page.waitForFunction(top => document.body.scrollTop === 60 &&
        document.activeElement.getBoundingClientRect().top === top - 60, initialTop);
      await page.evaluate(() => document.querySelector("#scroll-fixture").remove());
      await page.waitForFunction(top => document.activeElement.getBoundingClientRect().top === top, initialTop);
      const ime = await context.newCDPSession(page);
      const compose = text => ime.send("Input.imeSetComposition", {
        text, selectionStart:text.length, selectionEnd:text.length,
      });
      await compose("ni");
      await inputValue("");
      await compose("你好");
      await ime.send("Input.insertText", {text:"你好"});
      await inputValue("你好");
      await compose("hao");
      await compose("");
      await inputValue("你好");
      await ime.detach();
      await page.keyboard.press("Control+a");
      await page.keyboard.type("Plico");
      await inputValue("Plico");
      await page.waitForFunction(() => window.plico.semantics().some(n => n.label === "Clear filter"));
      // Seed a DOM paste with mixed Unicode, then use Chromium's own cut/copy/paste events.
      const payload = "漫画 · 第２卷 📚";
      await page.keyboard.press("Control+a");
      assert.equal(await page.locator("canvas").evaluate((canvas,text)=>{
        const data=new DataTransfer();
        data.setData("text/plain",text);
        return canvas.dispatchEvent(new ClipboardEvent("paste",{clipboardData:data,cancelable:true,bubbles:true}));
      },payload),false);
      await inputValue(payload);
      await page.keyboard.press("Control+a");
      assert.equal(await page.locator("canvas").evaluate(canvas=>{
        const data=new DataTransfer();
        data.setData("text/html","<b>rich text</b>");
        return canvas.dispatchEvent(new ClipboardEvent("paste",{clipboardData:data,cancelable:true,bubbles:true}));
      }),true);
      await inputValue(payload);
      await page.keyboard.press("Control+x");
      await inputValue("");
      await page.keyboard.press("Control+v");
      await inputValue(payload);
      await page.keyboard.press("Control+a");
      await page.keyboard.press("Control+c");
      await page.keyboard.press("End");
      await page.keyboard.press("Control+v");
      await inputValue(payload+payload);
      await click("Clear filter");
      await healthy();
      // The experiments section sits below the fold of the sidebar.
      const listPoint=await page.evaluate(()=>window.plico.semantics().find(n=>n.label==="Data table").bounds);
      // DOM line deltas have a CSS distance independent of devicePixelRatio.
      await page.evaluate(([x, y]) => document.querySelector("canvas").dispatchEvent(
        new WheelEvent("wheel", {clientX:x+40, clientY:y+10, deltaMode:1, deltaY:2, cancelable:true})
      ), listPoint);
      await page.waitForFunction(y => Math.abs(
        window.plico.semantics().find(n=>n.label==="Data table").bounds[1] - (y-32)
      ) < 0.1, listPoint[1]);
      await page.emulateMedia({reducedMotion:"reduce"});
      // Allow the media-query change listener to run before the next notch.
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
      const firstMoved = await page.evaluate(async ([x,y]) => {
        const top = () => window.plico.semantics().find(n => n.label === "Data table").bounds[1];
        const start = top();
        document.querySelector("canvas").dispatchEvent(new WheelEvent("wheel", {
          clientX:x+40, clientY:y, deltaMode:1, deltaY:-1, cancelable:true,
        }));
        for (let i=0; i<20; i++) {
          await new Promise(resolve => requestAnimationFrame(resolve));
          if (top() !== start) return top()-start;
        }
      }, listPoint);
      assert.ok(Math.abs(firstMoved-16)<0.1, "reduced-motion wheel should move one full line immediately");
      await page.emulateMedia({reducedMotion:"no-preference"});
      await page.mouse.move(listPoint[0]+40,listPoint[1]+10);
      await page.mouse.wheel(0,600);
      await page.waitForFunction(()=>window.plico.semantics().find(n=>n.label==="2048").bounds[3]<innerHeight-90);
      await click("2048");
      await page.waitForFunction(() => window.plico.state.router.current === "Game2048");
      await page.keyboard.press("ArrowLeft");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowRight");
      await page.waitForFunction(() => window.plico.state.game2048.tiles.length > 2);
      await page.mouse.wheel(0,-600);
      await page.waitForFunction(()=>window.plico.semantics().find(n=>n.label==="Layout").bounds[1]>50);
      await click("Lists");
      await page.waitForFunction(() => window.plico.state.router.current === "Lists");
      const row = await page.evaluate(() => window.plico.semantics().find(n => n.label === "Record 00001"));
      assert.ok(row, "first virtual row");
      await page.mouse.move(row.bounds[0]+30, row.bounds[1]+12);
      await page.mouse.wheel(0, 720);
      await page.waitForFunction(() => {
        const labels=window.plico.semantics().map(n=>n.label).filter(s=>s?.startsWith("Record "));
        return labels.length && !labels.includes("Record 00001");
      });
      await click("Motion");
      await page.waitForFunction(() => window.plico.state.router.current === "Motion");
      await healthy(); // WGSL fallback must not break the WebGL frame.
      await click("Layout");
      await page.waitForFunction(() => window.plico.state.router.current === "Layout");
      const mainX = await page.evaluate(() => window.plico.semantics().find(n=>n.label === "The remaining space").bounds[0]);
      await click("Animate layout");
      await page.waitForFunction(x => {
        const main=window.plico.semantics().find(n=>n.label === "The remaining space");
        return window.plico.state.expanded && main.bounds[0] > x+59;
      }, mainX);
      await page.setViewportSize({width:760,height:740});
      await page.waitForFunction(scale => document.querySelector("canvas").width===760*scale, scale);
      await healthy();
      await page.setViewportSize({width:1120,height:820});
      await page.waitForFunction(scale => document.querySelector("canvas").width===1120*scale, scale);
      await click("Animate layout");
      await page.waitForFunction(() => !window.plico.state.expanded);
      await healthy();
      const backend = await page.evaluate(() => window.plico.backend);
      if (backend === "webgl2") {
        await click("Glass");
        await page.waitForFunction(() => window.plico.state.router.current === "Glass");
        await click("Move background");
        await page.waitForTimeout(500); // let checkbox feedback settle, then compare a fixed panel
        await healthy();
        const glass = await page.screenshot({clip:{x:230,y:340,width:300,height:190}});
        await click("Blur background");
        await page.waitForTimeout(300);
        await healthy();
        const clear = await page.screenshot({clip:{x:230,y:340,width:300,height:190}});
        assert.ok(!glass.equals(clear), "WebGL backdrop blur changes panel pixels");
      }
      await click("Controls");
      await page.waitForFunction(() => window.plico.state.router.current === "Controls");
      await page.setViewportSize({width:900,height:700});
      await page.waitForFunction(scale => {
        const c=document.querySelector("canvas");
        return c.width===900*scale && c.height===700*scale;
      }, scale);
      // Let transitions settle, then prove the host stops scheduling frames.
      // Hover feedback and the delayed scrollbar fade have separate deadlines.
      await page.waitForFunction(()=>window.plico.nextFrameDelay===undefined);
      const frames=await page.evaluate(()=>window.plico.frames);
      await page.waitForTimeout(200);
      assert.equal(await page.evaluate(()=>window.plico.frames), frames, "idle rAF");
      await page.evaluate(() => {
        for (let i=0;i<100;i++) window.dispatchEvent(new Event("plico-wake"));
      });
      await page.waitForFunction(before => window.plico.frames > before, frames);
      await page.waitForTimeout(200);
      assert.equal(await page.evaluate(()=>window.plico.frames), frames+1, "wake burst rAF");
      await healthy();
      console.log(JSON.stringify({scale,backend,frames,page:"Controls",counter:3,checks:["pointer","theme","text-input","ime","clipboard","keyboard2048","virtual-scroll","shader-fallback",...(backend === "webgl2" ? ["backdrop-blur"] : []),"grid-layout","resize","idle"]}));
      await context.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode=1; });
