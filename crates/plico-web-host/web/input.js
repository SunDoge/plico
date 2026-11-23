// Shared canvas events for the resident gallery and replaceable UI host.
export function bindCanvasInput(canvas, app, invoke, editor) {
  const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
  const syncMotion = () => invoke(() => app.set_reduced_motion(motion.matches));
  motion.addEventListener("change", syncMotion);
  syncMotion();
  for (const type of ["pointerdown", "pointerup", "pointermove", "pointercancel", "pointerleave"]) {
    canvas.addEventListener(type, event => invoke(() => {
      if (type === "pointerdown") {
        canvas.focus({preventScroll:true});
        canvas.setPointerCapture(event.pointerId);
      }
      app.pointer(performance.now(), event);
      if (type === "pointerdown" && editor) {
        editor.sync();
        if (document.activeElement === editor.element) event.preventDefault();
      }
    }));
  }
  canvas.addEventListener("wheel", event => invoke(() => {
    if (app.wheel(performance.now(), event)) event.preventDefault();
  }), {passive: false});
  const targets = editor ? [canvas, editor.element] : [canvas];
  for (const target of targets) {
    for (const type of ["focus", "blur"]) {
      target.addEventListener(type, e => {
        if (!targets.includes(e.relatedTarget)) invoke(() => app.focus(performance.now(), type === "focus"));
      });
    }
    for (const type of ["copy", "cut"]) target.addEventListener(type, e => {
      const text = app.selected_text();
      if (!text || !e.clipboardData) return;
      e.clipboardData.setData("text/plain", text);
      e.preventDefault();
      if (type === "cut") invoke(() => app.cut_selection(performance.now()));
    });
    target.addEventListener("paste", e => invoke(() => {
      const text = e.clipboardData?.getData("text/plain");
      if (text && app.paste(performance.now(), text)) e.preventDefault();
    }));
  }
}
