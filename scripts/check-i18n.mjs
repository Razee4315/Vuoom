#!/usr/bin/env node
// Checks the Simplified Chinese dictionary against the source: every literal key passed to
// t() (or marked with tk(), for sentences the engine sends) must be in src/locales/zh-CN.json
// with the same {placeholders}. Unused dictionary keys are warnings. Exits 1 on a problem.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(root, "src");
const dict = JSON.parse(readFileSync(join(src, "locales", "zh-CN.json"), "utf8"));

const sources = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sources(path);
    return [".ts", ".tsx"].includes(extname(entry.name)) ? [path] : [];
  });

const CALL = /(?<![\w.])tk?\(\s*(["'])((?:\\.|(?!\1).)*)\1/g;
const unquote = (text, quote) => (quote === '"' ? JSON.parse(`"${text}"`) : text.replace(/\\(.)/g, "$1"));

const used = new Map();
for (const file of sources(src)) {
  const text = readFileSync(file, "utf8");
  for (const m of text.matchAll(CALL)) {
    const at = `${relative(root, file).replaceAll("\\", "/")}:${text.slice(0, m.index).split("\n").length}`;
    used.set(unquote(m[2], m[1]), at);
  }
}

const holes = (text) => [...text.matchAll(/\{\w+\}/g)].map((m) => m[0]).sort().join(" ");
const missing = [...used].filter(([key]) => !(key in dict));
const broken = [...used].filter(([key]) => key in dict && holes(key) !== holes(dict[key]));
const unused = Object.keys(dict).filter((key) => !used.has(key));

for (const key of unused) console.warn(`warning: unused key: ${JSON.stringify(key)}`);
for (const [key, at] of missing) console.error(`${at}: missing from zh-CN.json: ${JSON.stringify(key)}`);
for (const [key, at] of broken) console.error(`${at}: placeholders differ in zh-CN.json: ${JSON.stringify(key)}`);

console.log(`check-i18n: ${used.size} keys used, ${Object.keys(dict).length} in the dictionary`);
if (missing.length + broken.length > 0) process.exit(1);
