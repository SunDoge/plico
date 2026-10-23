import init, { SplitHost } from "./host/plico_web_host.js";
const canvas=document.querySelector("canvas"), status=document.querySelector("#status");
let host, queued=false, sequence=0, lastRender, lastRequest;
const transport={calls:0, milliseconds:0};
function measured(render) {
  lastRender=render;
  return request => {
    lastRequest=request;
    const start=performance.now();
    try {
      const reply=render(request);
      if(window.split) window.split.lastReply=reply;
      return reply;
    }
    finally {transport.calls++;transport.milliseconds+=performance.now()-start;}
  };
}
async function load(version) {
  const module=await import(`./ui-${version}/plico_web_demo.js?instance=${sequence++}`);
  await module.default();
  return measured(module.render);
}
function schedule() {
  if(queued) return;
  queued=true;
  requestAnimationFrame(time=>{
    queued=false;
    try {
      const rect=canvas.getBoundingClientRect();
      if(host.frame(time,Math.floor(rect.width),Math.floor(rect.height),devicePixelRatio)) schedule();
      window.split.frames++;
      window.split.stats=JSON.parse(host.stats());
    } catch(e) {window.split.error=String(e);status.textContent=String(e);}
  });
}
async function activate(version) {
  try {
    const module=await load(version);
    host.activate(module);
    status.textContent=`Active UI: ${version}`;
    schedule();
  } catch(e) {status.textContent=`Kept previous UI: ${e}`;throw e;}
}
try {
  await init();
  host=new SplitHost(canvas, await load("v1"));
  const font=await fetch("./DejaVuSans.ttf");
  if(!font.ok) throw new Error("font load failed");
  host.load_font(new Uint8Array(await font.arrayBuffer()));
  window.split={host,activate,schedule,frames:0,stats:null,error:null,transport,
    semantics:()=>JSON.parse(host.semantics()),
    benchmark:()=>{
      for(let i=0;i<20;i++) lastRender(lastRequest);
      const times=[];
      let reply;
      const batchStart=performance.now();
      for(let i=0;i<200;i++){
        const start=performance.now();
        reply=lastRender(lastRequest);
        times.push(performance.now()-start);
      }
      const total_ms=performance.now()-batchStart;
      times.sort((a,b)=>a-b);
      return {iterations:200,total_ms,mean_us:total_ms*1000/200,p50_ms:times[100],p95_ms:times[190],
        request_bytes:new TextEncoder().encode(lastRequest).length,
        reply_bytes:new TextEncoder().encode(reply).length};
    }};
  window.addEventListener("plico-wake",schedule);
  for(const button of document.querySelectorAll("[data-version]")) {
    button.addEventListener("click",()=>activate(button.dataset.version).catch(()=>{}));
  }
  document.querySelector("#service").onclick=()=>{host.advance_service();schedule();};
  for(const type of ["pointerdown","pointerup","pointermove","pointercancel","pointerleave"]) {
    canvas.addEventListener(type,e=>{
      if(type==="pointerdown"){canvas.focus();canvas.setPointerCapture(e.pointerId);}
      host.pointer(performance.now(),e);schedule();
    });
  }
  for(const type of ["keydown","keyup"]) canvas.addEventListener(type,e=>{
    if(host.key(performance.now(),e)) e.preventDefault();
    schedule();
  });
  new ResizeObserver(schedule).observe(canvas);
  window.addEventListener("resize",schedule);
  status.textContent="Active UI: v1";
  schedule();
} catch(e) {status.textContent=String(e);window.splitError=String(e);console.error(e);}
