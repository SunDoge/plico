import { bindTextInput } from "./text-input.js";
import { frameScheduler } from "./frames.js";
import { bindCanvasInput } from "./input.js";
import init, { Gallery } from "./pkg/plico_web_host.js";

const canvas = document.querySelector("canvas");
const error = document.querySelector("#error");
let app, editor, failed = false;
function fail(e) {
  failed = true;
  error.hidden = false;
  error.textContent = String(e?.stack ?? e);
  console.error(e);
}
const schedule = frameScheduler(time => {
  if (failed) return;
  try {
    const rect = canvas.getBoundingClientRect();
    const pending = app.frame(time, Math.max(1, Math.floor(rect.width)),
      Math.max(1, Math.floor(rect.height)), devicePixelRatio);
    editor.sync(rect);
    window.plico.frames++;
    window.plico.nextFrameDelay = pending ? app.next_frame_delay_ms() : undefined;
    return window.plico.nextFrameDelay;
  } catch (e) { fail(e); }
});
function invoke(fn) { try { fn(); schedule(); } catch(e) { fail(e); } }
try {
  await init();
  window.addEventListener("plico-wake", schedule);
  const renderer = new URL(location.href).searchParams.get("renderer") || "auto";
  await document.fonts.ready;
  app = await Gallery.create_wgpu(canvas, renderer);
  window.plico = { backend: app.renderer_name(), frames: 0,
    get state() { return this.frames ? JSON.parse(app.debug_state()) : null; },
    semantics: () => JSON.parse(app.semantics()), schedule };
  editor = bindTextInput(canvas, app, invoke);
  bindCanvasInput(canvas, app, invoke, editor);
  for (const target of [canvas, editor.element]) {
    for (const type of ["keydown", "keyup"]) target.addEventListener(type, e => invoke(() => {
      if (e.isComposing || e.keyCode === 229) return;
      if (app.key(performance.now(), e)) e.preventDefault();
      editor.sync();
    }));
  }
  canvas.addEventListener("contextmenu", e => e.preventDefault());
  new ResizeObserver(schedule).observe(canvas);
  window.addEventListener("resize", schedule);
  schedule();
} catch(e) { fail(e); }
