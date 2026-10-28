#!/usr/bin/env bun
/**
 * Maintain the pinned Lucide catalog; plico icons generates application selections.
 *
 *   bun scripts/icons.ts --fetch 02eb9161b57917ccaa262f1bf070507cddde3b08
 *   bun scripts/icons.ts            # regenerate constants from local SVGs
 *   bun scripts/icons.ts --check    # offline verification (hashes, license, Rust)
 *
 * SVGs are a generation cache, not a build prerequisite. Rust embeds their
 * exact bytes; LICENSE and SOURCE.json are committed alongside that output.
 */
import { createHash } from "node:crypto";
import { parseArgs } from "node:util";
import { mkdir, writeFile, unlink } from "node:fs/promises";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const VENDOR = join(ROOT, "crates/plico/assets/lucide");
const ASSETS = join(VENDOR, "icons");
const SOURCE = join(VENDOR, "SOURCE.json");
const REPO = "https://github.com/lucide-icons/lucide";
const sha256 = (data: Buffer | string) =>
  createHash("sha256").update(data).digest("hex");

/** A canonical Lucide SVG: a flat `icons/<kebab>.svg` file with an <svg> root. */
interface IconFile {
  name: string;
  data: Buffer;
}

/**
 * Tarball entries we accept: only flat `icons/*.svg` and a top-level LICENSE.
 * Everything else in the upstream archive is ignored — we never extract
 * arbitrary paths from downloaded content.
 */
async function readArchive(
  archive: Buffer,
): Promise<{ files: IconFile[]; license: Buffer | null }> {
  // Minimal gzipped-tar reader: Bun supports node:zlib.
  const zlib = await import("node:zlib");
  const tar = zlib.gunzipSync(archive);
  const files: IconFile[] = [];
  let license: Buffer | null = null;
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    const nameField = header.subarray(0, 100).toString("utf-8");
    if (!nameField.trim()) break; // end-of-archive marker
    const sizeField = header.subarray(124, 136).toString("utf-8").trim();
    const size = parseInt(sizeField || "0", 8);
    const type = header[156];
    const data = tar.subarray(offset + 512, offset + 512 + size);
    offset += 512 + Math.ceil(size / 512) * 512;
    if (type !== 0x30 && type !== 0) continue; // regular files only
    // Tar name fields are NUL-padded; strip before matching.
    const parts = nameField
      .replace(/\0/g, "")
      .split("/")
      .filter((part) => part.length > 0);
    if (parts.length === 2 && parts[1] === "LICENSE") {
      license = Buffer.from(data);
    } else if (
      parts.length === 3 &&
      parts[1] === "icons" &&
      parts[2].endsWith(".svg") &&
      // Long-name prefix entries (pax) carry the real name in the payload;
      // canonical Lucide names are short enough that we reject them instead.
      !nameField.startsWith("././@PaxHeader")
    ) {
      const name = parts[2];
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*\.svg$/.test(name)) {
        throw new Error(`Unexpected icon filename: ${name}`);
      }
      const root = data.toString("utf-8").trimStart().slice(0, 200);
      if (!/<svg[\s>]/.test(root)) {
        throw new Error(`Invalid SVG root: ${name}`);
      }
      files.push({ name, data: Buffer.from(data) });
    }
  }
  return { files, license };
}

async function fetch(commit: string) {
  if (!/^[0-9a-f]{40}$/.test(commit)) {
    throw new Error("--fetch requires a full 40-character commit SHA");
  }
  const url = `https://codeload.github.com/lucide-icons/lucide/tar.gz/${commit}`;
  console.log(`Fetching ${url}`);
  const archive = Buffer.from(
    await (await globalThis.fetch(url)).arrayBuffer(),
  );
  const { files, license } = await readArchive(archive);
  if (files.length === 0 || !license) {
    throw new Error("Archive did not contain Lucide icons and LICENSE");
  }
  interface Manifest {
    files: Record<string, string>;
  }
  const previous: Manifest["files"] = SOURCE.endsWith("SOURCE.json") &&
      await Bun.file(SOURCE).exists()
    ? (JSON.parse(await Bun.file(SOURCE).text()) as Manifest).files
    : {};
  await mkdir(ASSETS, { recursive: true });
  for (const name of Object.keys(previous)) {
    if (!files.some((file) => file.name === name)) {
      await unlink(join(ASSETS, name)).catch(() => {});
    }
  }
  for (const file of files) {
    await writeFile(join(ASSETS, file.name), file.data);
  }
  await writeFile(join(VENDOR, "LICENSE"), license);
  const manifest = {
    repository: REPO,
    commit,
    archive_url: url,
    archive_sha256: sha256(archive),
    icon_count: files.length,
    license_sha256: sha256(license),
    files: Object.fromEntries(
      [...files]
        .sort((a, b) => (a.name < b.name ? -1 : 1))
        .map((file) => [file.name, sha256(file.data)]),
    ),
  };
  await writeFile(SOURCE, JSON.stringify(manifest, null, 2) + "\n");
}

async function generate(check: boolean) {
  // The Rust CLI is the only generator; the maintenance script fetches the
  // upstream catalog and delegates the four framework control glyphs to it.
  const args = ["cargo", "run", "--quiet", "-p", "plico-cli", "--", "icons"];
  if (check) {
    args.push("check", "--output", "crates/plico/src/icons.rs");
  } else {
    args.push("add", "lucide:check", "lucide:chevron-down", "lucide:chevron-left", "lucide:chevron-right", "lucide:x",
      "--output", "crates/plico/src/icons.rs", "--rust-crate", "crate", "--source", VENDOR);
  }
  const child = Bun.spawn(args, { cwd: ROOT, stdout: "inherit", stderr: "inherit" });
  if (await child.exited !== 0) throw new Error("plico icons failed");
}

// Bun may place the script path in argv[1] or omit it; filter flags robustly.
const argv = process.argv.slice(2).filter(
  (arg) => !arg.endsWith("scripts/icons.ts") && arg !== "scripts/icons.ts",
);
const { values } = parseArgs({
  args: argv,
  options: {
    fetch: { type: "string" },
    check: { type: "boolean" },
  },
});
if (values.fetch && values.check) {
  throw new Error("--fetch and --check are mutually exclusive");
}
try {
  if (values.fetch) await fetch(values.fetch);
  await generate(values.check === true);
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
