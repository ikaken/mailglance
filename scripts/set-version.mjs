// Sets the app version in every file that holds it.   node scripts/set-version.mjs 0.2.0
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: node scripts/set-version.mjs <x.y.z>");
  process.exit(1);
}

const path = (p) => new URL(`../${p}`, import.meta.url);
const edit = (p, fn) => {
  const before = readFileSync(path(p), "utf8");
  const after = fn(before);
  if (after === before) console.log(`unchanged  ${p}`);
  else {
    writeFileSync(path(p), after);
    console.log(`updated    ${p}`);
  }
};

// Replace only the first top-level "version" so dependency versions stay untouched.
const json = (text) => text.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`);

edit("package.json", json);
edit("src-tauri/tauri.conf.json", json);
edit("package-lock.json", (t) =>
  // the root "version" and packages[""].version
  t.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`).replace(/("packages":\s*\{\s*"":\s*\{[^}]*?"version":\s*")[^"]+(")/, `$1${version}$2`),
);
edit("src-tauri/Cargo.toml", (t) => t.replace(/^(version\s*=\s*")[^"]+(")/m, `$1${version}$2`));
edit("src-tauri/Cargo.lock", (t) => t.replace(/(name = "mailglance"\r?\nversion = ")[^"]+(")/, `$1${version}$2`));
