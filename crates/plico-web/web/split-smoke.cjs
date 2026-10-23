const assert=require("node:assert/strict");
const {chromium}=require("playwright");
(async()=>{
 const browser=await chromium.launch({headless:true,
  ...(process.env.CHROMIUM_PATH?{executablePath:process.env.CHROMIUM_PATH}:{}),
  args:["--no-sandbox","--enable-unsafe-swiftshader","--use-angle=swiftshader"]});
 try {
  const page=await browser.newPage({viewport:{width:1000,height:650}});
  const errors=[], downloads=[];
  page.on("pageerror",e=>errors.push(String(e)));
  page.on("request",r=>{if(r.url().endsWith(".wasm"))downloads.push(r.url())});
  await page.goto(process.env.PLICO_SPLIT_URL||"http://127.0.0.1:8081");
  await page.waitForFunction(()=>window.split?.stats || window.splitError);
  assert.equal(await page.evaluate(()=>window.splitError),undefined);
  async function stats(){return page.evaluate(()=>JSON.parse(window.split.host.stats()));}
  // Canvas Y depends on header height, so derive its viewport offset.
  async function increment(){
   const n=await page.evaluate(()=>window.split.semantics().find(n=>n.label==="Increment"));
   const c=await page.locator("canvas").boundingBox();
   await page.mouse.click(c.x+(n.bounds[0]+n.bounds[2])/2,c.y+(n.bounds[1]+n.bounds[3])/2);
  }
  for(let i=0;i<3;i++)await increment();
  await page.waitForFunction(()=>window.split.stats.state.count===3);
  await page.evaluate(()=>{window.split.host.advance_service();window.split.schedule()});
  await page.waitForFunction(()=>window.split.stats.service.progress===1);
  const before=await stats();
  await page.evaluate(()=>window.split.activate("v2"));
  await page.waitForFunction(()=>window.split.stats.reloads===1);
  const compatible=await stats();
  assert.equal(compatible.state.count,3);
  assert.deepEqual(compatible.service,before.service);
  assert.equal(compatible.font_loads,1);
  assert.equal(compatible.created,0);
  assert.equal(compatible.svg_parses,0);
  assert.ok(compatible.text_cache[0]>=before.text_cache[0]);
  assert.ok(compatible.text_cache[1]>=before.text_cache[1]);
  for(const invalid of ["trap","json","protocol","tree","commands"]){
   const rejected=await page.evaluate(kind=>{
    try{
     window.split.host.activate(request=>{
      if(kind==="trap")throw new Error("fixture trap");
      if(kind==="json")return "{";
      // Produce a real candidate through the active UI module's protocol.
      const r=JSON.parse(request);
      const frame=JSON.parse(window.split.lastReply);
      frame.state=r.state;
      frame.version=999;
      if(kind==="tree"||kind==="commands"){
       // Reuse a valid captured reply, then change only the field under test.
       const original=JSON.parse(window.split.lastReply);
       if(kind==="tree") original.nodes.push(structuredClone(original.nodes[0]));
       else original.commands.push({unexpected:true});
       return JSON.stringify(original);
      }
      return JSON.stringify(frame);
     });
     return false;
    }catch{return true;}
   },invalid);
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
  assert.equal(reset.font_loads,1);
  await page.evaluate(()=>{window.split.host.advance_service();window.split.schedule()});
  await page.waitForFunction(()=>window.split.stats.service.progress===2);
  assert.deepEqual(errors,[]);
  assert.equal(await page.evaluate(()=>window.split.error),null);
  assert.equal(downloads.filter(u=>u.includes("/host/")).length,1);
  console.log(JSON.stringify({before,compatible,reset,downloads,
    transport:await page.evaluate(()=>window.split.transport),
    benchmark:await page.evaluate(()=>window.split.benchmark())}));
 }finally{await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
