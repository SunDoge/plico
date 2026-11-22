import { frameScheduler } from "./frames.js";
import { bindCanvasInput } from "./input.js";
import init, { SplitHost } from "./host/plico_web_host.js";
import { openUiModule } from "./module.mjs";
const canvas=document.querySelector("canvas"), status=document.querySelector("#status");
let host;
const transport={calls:0, milliseconds:0};
function measured(render) {
  return request => {
    const start=performance.now();
    try {
      const reply=render(request);
      transport.last_reply_bytes=reply.byteLength;
      return reply;
    }
    finally {transport.calls++;transport.milliseconds+=performance.now()-start;}
  };
}
async function load(version) {
  const response=await fetch(`./ui-${version}.wasm`, {cache:"no-cache"});
  if(!response.ok) throw new Error(`Could not load ${version}: ${response.status}`);
  const render=await openUiModule(await response.arrayBuffer(), SplitHost.abi_version(), SplitHost.buffer_capacity());
  return measured(render);
}
const schedule=frameScheduler(time=>{
  try {
    const rect=canvas.getBoundingClientRect();
    const pending=host.frame(time,Math.floor(rect.width),Math.floor(rect.height),devicePixelRatio);
    window.split.frames++;
    if(pending) return host.next_frame_delay_ms();
  } catch(e) {window.split.error=String(e);status.textContent=String(e);}
});
async function activate(version) {
  try {
    const module=await load(version);
    host.activate(module);
    status.textContent=`Active UI: ${version}`;
    schedule();
  } catch(e) {status.textContent=`Kept previous UI: ${e}`;throw e;}
}
async function nextBuild(revision) {
  const response = await fetch(`./__changes?${revision}`, {cache:"no-store"});
  if (!response.ok) throw new Error(response.statusText);
  const build = await response.json();
  const details = document.querySelector("#diagnostics");
  if (build.error !== details.querySelector("pre").textContent) {
    details.querySelector("pre").textContent = build.error;
    details.hidden = !build.error;
    details.open = Boolean(build.error);
  }
  return build;
}
async function followBuilds(revision) {
  for (;;) {
    try {
      const build = await nextBuild(revision);
      if (build.revision === revision) continue;
      revision = build.revision;
      if (build.error) status.textContent = "Build failed";
      else await activate("dev");
    } catch (error) {
      status.textContent = String(error);
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }
}
try {
  await init();
  await document.fonts.ready;
  const dev = new URL(location.href).searchParams.has("dev");
  let revision = 0;
  if (dev) {
    for (const button of document.querySelectorAll("[data-version]")) button.hidden = true;
    status.textContent = "Building UI…";
    for (;;) {
      const build = await nextBuild(revision);
      revision = build.revision;
      if (revision && !build.error) break;
      if (build.error) status.textContent = "Build failed";
    }
  }
  host=await SplitHost.create(canvas, await load(dev ? "dev" : "v1"));
  window.split={host,activate,schedule,frames:0,error:null,transport,
    get stats(){return this.frames ? JSON.parse(host.stats()) : null;},
    semantics:()=>JSON.parse(host.semantics()),
  };
  window.addEventListener("plico-wake",schedule);
  for(const button of document.querySelectorAll("[data-version]")) {
    button.addEventListener("click",()=>activate(button.dataset.version).catch(()=>{}));
  }
  document.querySelector("#service").onclick=()=>{host.advance_service();schedule();};
  bindCanvasInput(canvas, host, fn => { fn(); schedule(); });
  for(const type of ["keydown","keyup"]) canvas.addEventListener(type,e=>{
    if(host.key(performance.now(),e)) e.preventDefault();
    schedule();
  });
  new ResizeObserver(schedule).observe(canvas);
  window.addEventListener("resize",schedule);
  status.textContent=dev ? "Active UI: dev" : "Active UI: v1";
  schedule();
  if (dev) void followBuilds(revision);
} catch(e) {status.textContent=String(e);window.splitError=String(e);console.error(e);}
