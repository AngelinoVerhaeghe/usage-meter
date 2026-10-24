// Set the version everywhere it lives:  node scripts/bump.mjs 0.1.1
// Then commit, `git tag v0.1.1`, push main and the tag, and CI publishes it.
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("usage: node scripts/bump.mjs 1.2.3");
  process.exit(1);
}

const edit = (path, fn) => writeFileSync(path, fn(readFileSync(path, "utf8")));
edit("package.json", (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`));
edit("src-tauri/tauri.conf.json", (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`));
edit("package-lock.json", (s) => s.replace(/("name": "usage-meter",\s+"version": )"[^"]+"/g, `$1"${version}"`));
edit("src-tauri/Cargo.toml", (s) => s.replace(/^version = "[^"]+"/m, `version = "${version}"`));
console.log(`version set to ${version}; now commit, tag v${version} and push the tag`);
