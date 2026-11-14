// Verifies that every file holding the app version agrees (and matches a release tag, if given).
//   node scripts/check-version.mjs          # files agree with each other
//   node scripts/check-version.mjs v0.2.0   # ...and with the tag
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

const versions = {
  "package.json": JSON.parse(read("package.json")).version,
  "package-lock.json": JSON.parse(read("package-lock.json")).version,
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  "src-tauri/Cargo.toml": read("src-tauri/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1],
  "src-tauri/Cargo.lock": read("src-tauri/Cargo.lock").match(/name = "mailglance"\r?\nversion = "([^"]+)"/)?.[1],
};

const tag = process.argv[2]?.replace(/^v/, "");
const expected = tag ?? versions["package.json"];
const bad = Object.entries(versions).filter(([, v]) => v !== expected);

for (const [file, v] of Object.entries(versions)) console.log(`${v ?? "(not found)"}\t${file}`);
if (bad.length) {
  console.error(`\nVersion mismatch: expected ${expected} (${tag ? "from the tag" : "package.json"}) in ${bad.map(([f]) => f).join(", ")}`);
  console.error("Fix it with: node scripts/set-version.mjs <x.y.z>");
  process.exit(1);
}
console.log(`\nAll versions are ${expected}.`);
