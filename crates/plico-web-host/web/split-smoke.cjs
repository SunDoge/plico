const assert=require("node:assert/strict");
const {chromium}=require("playwright");
(async()=>{
 const browser=await chromium.launch({headless:true,
  ...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),
  args:["--no-sandbox","--enable-unsafe-swiftshader","--use-angle=swiftshader"]});
 const fs=require("node:fs/promises");
 const path=require("node:path");
 const fixtures=await fs.mkdtemp(path.join(require("node:os").tmpdir(),"plico-rejected-"));
 try {
  require("node:child_process").execFileSync("cargo",["run","--locked","--quiet","-p","web-demo","--example","rejected_frames","--",fixtures],{cwd:path.resolve(__dirname,"../../..")});
  const page=await browser.newPage({viewport:{width:1000,height:650}});
  const errors=[], downloads=[];
  page.on("pageerror",e=>errors.push(String(e)));
  page.on("request",r=>{if(r.url().endsWith(".wasm"))downloads.push(r.url())});
  await page.goto(process.env.PLICO_SPLIT_URL||"http://127.0.0.1:8081");
  await page.waitForFunction(()=>window.split?.stats || window.splitError || window.split?.error);
  assert.equal(await page.evaluate(()=>window.splitError),undefined);
  async function stats(){return page.evaluate(()=>JSON.parse(window.split.host.stats()));}
  // Canvas Y depends on header height, so derive its viewport offset.
  async function increment(){
   const n=await page.evaluate(()=>window.split.semantics().find(n=>n.label==="Increment"));
   const c=await page.locator("canvas").boundingBox();
   await page.mouse.click(c.x+(n.bounds[0]+n.bounds[2])/2,c.y+(n.bounds[1]+n.bounds[3])/2);
  }
  for(let i=0;i<2;i++)await increment();
  // The application binds a physical code. Holding the key must count once.
  const keyBytes=(await stats()).output_bytes;
  await page.keyboard.down("Alt");
  await page.keyboard.down("ArrowUp");
  await page.keyboard.down("ArrowUp");
  await page.keyboard.up("ArrowUp");
  await page.keyboard.up("Alt");
  await page.waitForFunction(()=>window.split.stats.state.count===3);
  assert.ok((await stats()).output_bytes-keyBytes < 4096, "Shortcut probes should retain their delta baseline");
  const composing=await page.locator("canvas").evaluate(canvas=>{
   const before=window.split.host.stats();
   const propagate=canvas.dispatchEvent(new KeyboardEvent("keydown", {key:"ArrowUp",code:"ArrowUp",altKey:true,isComposing:true,bubbles:true,cancelable:true}));
   return propagate && JSON.parse(before).calls === JSON.parse(window.split.host.stats()).calls;
  });
  assert.ok(composing, "IME keydowns must stay available to the browser");
  await page.evaluate(()=>{window.split.host.advance_service();window.split.schedule()});
  await page.waitForFunction(()=>window.split.stats.service.progress===1);
  const before=await stats();
  await page.evaluate(()=>window.split.activate("v2"));
  await page.waitForFunction(()=>window.split.stats.reloads===1);
  const compatible=await stats();
  assert.equal(compatible.state.count,3);
  assert.deepEqual(compatible.service,before.service);
  assert.equal(compatible.font_loads,0);
  assert.equal(compatible.created,0);
  assert.equal(compatible.svg_parses,0);
  assert.ok(compatible.text_cache[0]>=before.text_cache[0]);
  assert.ok(compatible.text_cache[1]>=before.text_cache[1]);
  for(const invalid of ["trap","type","archive","oversized","protocol","tree"]){
   const data=["protocol","tree"].includes(invalid) ? [...await fs.readFile(path.join(fixtures,invalid))] : [];
   const rejected=await page.evaluate(({kind,data})=>{
    try{
     window.split.host.activate(()=>{
      if(kind==="trap") throw new Error("fixture trap");
      if(kind==="type") return "{}";
      if(kind==="oversized") return new Uint8Array(1024*1024+1);
      return Uint8Array.from(data);
     });
     return false;
    }catch{return true;}
   },{kind:invalid,data});
   assert.ok(rejected,invalid);
   assert.equal((await stats()).reloads,1);
   assert.equal((await stats()).state.count,3);
  }
  await increment();
  await page.waitForFunction(()=>window.split.stats.state.count===4);
  await page.evaluate(()=>window.split.activate("reset"));
  await page.waitForFunction(()=>window.split.stats.reloads===2);
  const reset=await stats();
  assert.equal(reset.state.count,0);
  assert.ok(reset.reset);
  assert.deepEqual(reset.service,before.service);
  assert.equal(reset.font_loads,0);
  await page.evaluate(()=>{window.split.host.advance_service();window.split.schedule()});
  await page.waitForFunction(()=>window.split.stats.service.progress===2);
  await page.waitForFunction(()=>window.split.semantics().some(n=>n.label?.includes("progress 2")));
  const updateBytes=await page.evaluate(()=>window.split.transport.last_reply_bytes);
  assert.ok(updateBytes < 512, "A service text change should fit in a small patch: "+updateBytes);
  // A short viewport puts the actions below the fold. Scroll them into view,
  // click at their presented position, then return to the title.
  await page.setViewportSize({width:700,height:220});
  await page.waitForFunction(()=>document.querySelector("canvas").height===160);
  const startY=await page.evaluate(()=>window.split.semantics().find(n=>n.label==="Increment").bounds[1]);
  const canvas=await page.locator("canvas").boundingBox();
  await page.mouse.move(canvas.x+50,canvas.y+60);
  await page.mouse.wheel(0,240);
  await page.waitForFunction(y=>window.split.semantics().find(n=>n.label==="Increment")?.bounds[1]<y-20,startY);
  await increment();
  await page.waitForFunction(()=>window.split.stats.state.count===1);
  await page.mouse.wheel(0,-240);
  await page.waitForFunction(()=>window.split.semantics().find(n=>n.label?.startsWith("Replaceable UI"))?.bounds[1]>=0);
  assert.deepEqual(errors,[]);
  assert.equal(await page.evaluate(()=>window.split.error),null);
  assert.equal(downloads.filter(u=>u.includes("/host/")).length,1);
  console.log(JSON.stringify({before,compatible,reset,downloads,
    transport:await page.evaluate(()=>window.split.transport)}));
 }finally{await browser.close();await fs.rm(fixtures,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exitCode=1});
