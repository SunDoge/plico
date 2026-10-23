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
      await page.waitForFunction(() => window.plico.state.page === "Controls");
      for (let i=0;i<3;i++) await click("Increment");
      await page.waitForFunction(() => window.plico.state.count === 3);
      await click("Dark theme");
      await page.waitForFunction(() => window.plico.state.dark);
      await click("2048");
      await page.waitForFunction(() => window.plico.state.page === "Game2048");
      await page.keyboard.press("ArrowLeft");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowRight");
      await page.waitForFunction(() => window.plico.state.game2048.tiles.length > 2);
      await click("Lists");
      await page.waitForFunction(() => window.plico.state.page === "Lists");
      const row = await page.evaluate(() => window.plico.semantics().find(n => n.label === "Record 00001"));
      assert.ok(row, "first virtual row");
      await page.mouse.move(row.bounds[0]+30, row.bounds[1]+12);
      await page.mouse.wheel(0, 720);
      await page.waitForFunction(() => {
        const labels=window.plico.semantics().map(n=>n.label).filter(s=>s?.startsWith("Record "));
        return labels.length && !labels.includes("Record 00001");
      });
      await click("Motion");
      await page.waitForFunction(() => window.plico.state.page === "Motion");
      await healthy(); // WGSL fallback must not break the WebGL frame.
      await click("Layout");
      await page.waitForFunction(() => window.plico.state.page === "Layout");
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
      await click("Glass");
      await page.waitForFunction(() => window.plico.state.page === "Glass");
      await click("Move background");
      await page.waitForTimeout(500); // let checkbox feedback settle, then compare a fixed panel
      await healthy();
      const glass = await page.screenshot({clip:{x:230,y:340,width:300,height:190}});
      await click("Blur background");
      await page.waitForTimeout(300);
      await healthy();
      const clear = await page.screenshot({clip:{x:230,y:340,width:300,height:190}});
      assert.ok(!glass.equals(clear), "WebGL backdrop blur changes panel pixels");
      await click("Controls");
      await page.waitForFunction(() => window.plico.state.page === "Controls");
      await page.setViewportSize({width:900,height:700});
      await page.waitForFunction(scale => {
        const c=document.querySelector("canvas");
        return c.width===900*scale && c.height===700*scale;
      }, scale);
      // Let transitions settle, then prove the host stops scheduling frames.
      await page.waitForTimeout(800);
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
      console.log(JSON.stringify({scale,frames,page:"Controls",counter:3,checks:["pointer","theme","keyboard2048","virtual-scroll","shader-fallback","backdrop-blur","grid-layout","resize","idle"]}));
      await context.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode=1; });
