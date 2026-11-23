// The browser owns only the transient composition. Committed text and the
// selection remain in Plico, including when the DOM input receives focus.
export function bindTextInput(canvas, app, invoke) {
  const element = document.createElement("input");
  element.tabIndex = -1;
  element.hidden = true;
  element.autocomplete = "off";
  element.spellcheck = false;
  element.setAttribute("autocapitalize", "off");
  element.setAttribute("aria-label", "Text input");
  element.style.cssText = "position:fixed;width:1px;padding:0;border:0;opacity:0;pointer-events:none;font-size:16px";
  canvas.after(element);
  let client;
  let composing = false;

  function sync(canvasRect) {
    if (document.activeElement !== canvas && document.activeElement !== element) return;
    const anchor = app.input_anchor();
    if (!anchor) {
      client = undefined;
      composing = false;
      element.value = "";
      if (document.activeElement === element) canvas.focus({preventScroll:true});
      element.hidden = true;
      return;
    }
    try {
      element.hidden = false;
      if (client !== anchor.client) {
        if (document.activeElement === element) canvas.focus({preventScroll:true});
        composing = false;
        element.value = "";
        client = anchor.client;
        element.type = anchor.secret ? "password" : "text";
      }
      const rect = canvasRect ?? canvas.getBoundingClientRect();
      element.style.left = (rect.left + anchor.x) + "px";
      element.style.top = (rect.top + anchor.y) + "px";
      element.style.height = anchor.height + "px";
      if (document.activeElement !== element) element.focus({preventScroll:true});
    } finally { anchor.free(); }
  }

  // Scrolling an enclosing page moves the canvas without resizing or repainting it.
  window.addEventListener("scroll", () => {
    if (document.activeElement === element) sync();
  }, {capture:true, passive:true});

  element.addEventListener("compositionstart", () => { composing = true; });
  element.addEventListener("compositionupdate", e => {
    if (composing) invoke(() => app.composition(performance.now(), client, e.data, false));
  });
  element.addEventListener("compositionend", e => {
    if (composing) invoke(() => app.composition(performance.now(), client, e.data, true));
    composing = false;
    element.value = "";
  });
  element.addEventListener("beforeinput", e => {
    if (composing || e.isComposing || e.inputType.includes("Composition")) return;
    e.preventDefault();
    if (e.inputType === "insertText" && e.data) invoke(() => app.text(performance.now(), e.data));
  });
  element.addEventListener("input", () => {
    if (!composing) element.value = "";
  });
  element.addEventListener("blur", () => {
    if (composing) invoke(() => app.composition(performance.now(), client, "", true));
    composing = false;
    element.value = "";
  });
  return {element, sync};
}
