import init, { Gallery } from "./pkg/plico_web.js";

const canvas = document.querySelector("canvas");
const error = document.querySelector("#error");
let app, queued = false, failed = false;
function fail(e) {
  failed = true;
  error.hidden = false;
  error.textContent = String(e?.stack ?? e);
  console.error(e);
}
function schedule() {
  if (queued || failed) return;
  queued = true;
  requestAnimationFrame(time => {
    queued = false;
    try {
      const rect = canvas.getBoundingClientRect();
      if (app.frame(time, Math.max(1, Math.floor(rect.width)),
          Math.max(1, Math.floor(rect.height)), devicePixelRatio)) schedule();
      window.plico.frames++;
      window.plico.state = JSON.parse(app.debug_state());
    } catch (e) { fail(e); }
  });
}
function invoke(fn) { try { fn(); schedule(); } catch(e) { fail(e); } }
try {
  await init();
  window.addEventListener("plico-wake", schedule);
  app = new Gallery(canvas);
  const response = await fetch("./DejaVuSans.ttf");
  if (!response.ok) throw new Error("Cannot load bundled demo font");
  app.load_font(new Uint8Array(await response.arrayBuffer()));
  window.plico = { frames: 0, state: null, semantics: () => JSON.parse(app.semantics()), schedule };
  for (const type of ["pointerdown", "pointerup", "pointermove", "pointercancel", "pointerleave"]) {
    canvas.addEventListener(type, e => invoke(() => {
      if (type === "pointerdown") { canvas.focus(); canvas.setPointerCapture(e.pointerId); }
      app.pointer(performance.now(), e);
    }));
  }
  canvas.addEventListener("wheel", e => invoke(() => {
    if (app.wheel(performance.now(), e)) e.preventDefault();
  }), {passive:false});
  for (const type of ["keydown", "keyup"]) canvas.addEventListener(type, e => invoke(() => {
    if (e.isComposing) return;
    const handled = app.key(performance.now(), e);
    const copy = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && app.selected_text();
    if (handled && !copy) e.preventDefault();
    if (type === "keydown" && !handled && !e.ctrlKey && !e.metaKey && !e.altKey &&
        [...e.key].length === 1) {
      app.text(performance.now(), e.key);
      e.preventDefault();
    }
  }));
  canvas.addEventListener("copy", e => {
    const text = app.selected_text();
    if (text && e.clipboardData) {
      e.clipboardData.setData("text/plain", text);
      e.preventDefault();
    }
  });
  canvas.addEventListener("focus", () => invoke(() => app.focus(performance.now(), true)));
  canvas.addEventListener("blur", () => invoke(() => app.focus(performance.now(), false)));
  canvas.addEventListener("contextmenu", e => e.preventDefault());
  new ResizeObserver(schedule).observe(canvas);
  window.addEventListener("resize", schedule);
  schedule();
} catch(e) { fail(e); }
