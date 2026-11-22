// Run after building reader-ui (tree) and reader-host (tree,wasm-host).
const { chromium } = require("playwright");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs/promises");
const os = require("node:os");
const path = require("node:path");

async function main() {
  const [host, guest] = process.argv.slice(2);
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "plico-reader-"));
  let browser;
  try {
    const [version, capacity] = execFileSync(host, ["BROWSER", directory],
      { encoding: "utf8", env: { ...process.env, PLICO_WASM_MODULE: guest } }).trim().split(/\s+/).map(Number);
    const files = new Map([
      ["/reader.wasm", guest],
      ["/module.mjs", path.resolve(__dirname, "../../crates/plico-wasm/web/module.mjs")],
    ]);
    for (const name of await fs.readdir(directory)) files.set("/" + name, path.join(directory, name));
    browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH });
    const page = await browser.newPage();
    await page.route("http://reader.test/**", async route => {
      const name = new URL(route.request().url()).pathname;
      if (name === "/") return route.fulfill({contentType: "text/html", body: "<title>Reader parity</title>"});
      if (!files.has(name)) return route.abort();
      return route.fulfill({path: files.get(name), contentType: name.endsWith(".mjs") ?
        "text/javascript" : "application/octet-stream"});
    });
    await page.goto("http://reader.test/");
    const lengths = await page.evaluate(async ({version, capacity}) => {
      const {openUiModule} = await import("/module.mjs");
      const bytes = async url => new Uint8Array(await (await fetch(url)).arrayBuffer());
      const call = await openUiModule(await bytes("/reader.wasm"), version, capacity);
      const sizes = [];
      for (let i = 0; i < 4; i++) {
        const request = await bytes("/" + i + ".request");
        const expected = await bytes("/" + i + ".reply");
        const actual = call(request);
        if (actual.length !== expected.length || !actual.every((v, j) => v === expected[j])) {
          throw new Error("Reader frame diverged at step " + i);
        }
        sizes.push([request.length, actual.length]);
      }
      return sizes;
    }, {version, capacity});
    console.log("Chrome matched all Wasmtime reader frames (request/reply bytes)", lengths);
  } finally {
    try { await browser?.close(); }
    finally { await fs.rm(directory, { recursive: true, force: true }); }
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
