// `ebb` CLI'sini Tauri sidecar olarak hazırlar: src-tauri/binaries/ebb-<triple>[.exe]
// Kullanım: node scripts/sidecar.mjs [debug|release]
import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");
const profile = process.argv[2] === "release" ? "release" : "debug";
const host = execSync("rustc -vV").toString().match(/host: (.+)/)?.[1]?.trim();
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const ext = triple.includes("windows") ? ".exe" : "";

execSync(`cargo build -p ebb-cli ${profile === "release" ? "--release" : ""} --target ${triple}`, { cwd: root, stdio: "inherit" });
const from = path.join(root, "target", triple, profile, `ebb${ext}`);
const dir = path.join(here, "../src-tauri/binaries");
fs.mkdirSync(dir, { recursive: true });
fs.copyFileSync(from, path.join(dir, `ebb-${triple}${ext}`));
console.log(`sidecar: ebb-${triple}${ext} (${profile})`);
