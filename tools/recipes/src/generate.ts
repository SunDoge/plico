import { mkdir, writeFile, rename } from "node:fs/promises";
import { dirname, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { validate } from "./index.ts";

const [entry, destination, ...extra] = process.argv.slice(2);
if (!entry || !destination || extra.length) throw new Error("usage: node tools/recipes/src/generate.ts <suite.ts> <plan.json>");
const plan = (await import(pathToFileURL(resolve(entry)).href)).default;
validate(plan);
for (const item of plan.cases) for (const step of item.steps) {
  if (step.source) step.source.file = relative(process.cwd(), step.source.file);
}
const output = resolve(destination);
await mkdir(dirname(output), { recursive: true });
const temporary = `${output}.${process.pid}.tmp`;
await writeFile(temporary, `${JSON.stringify(plan, null, 2)}\n`);
await rename(temporary, output);
console.log(`Generated ${plan.cases.length} recipes: ${output}`);
