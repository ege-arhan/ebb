// Arayüz duman testi: mock verilerle her ekranı açar; konsol hatası, taşan sayfa ya da boş ekran varsa başarısız olur.
import puppeteer from "puppeteer-core";
import { spawn } from "node:child_process";

import fs from "node:fs";
const chrome = process.env.CHROME_PATH || ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/google-chrome", "/usr/bin/chromium"].find((p) => fs.existsSync(p));
const server = spawn("pnpm", ["exec", "vite", "preview", "--port", "4179", "--strictPort"], { stdio: "ignore" });
await new Promise((r) => setTimeout(r, 2500));
let failed = 0;
const browser = await puppeteer.launch({ executablePath: chrome, headless: "new", args: ["--no-sandbox"], defaultViewport: { width: 1280, height: 860 } });
try {
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await page.goto("http://localhost:4179/?nowelcome&lang=en", { waitUntil: "networkidle0" });
  const ids = await page.$$eval(".rail nav button", (b) => b.length);
  for (let i = 0; i < ids; i++) {
    await page.evaluate((n) => document.querySelectorAll(".rail nav button")[n].click(), i);
    await new Promise((r) => setTimeout(r, 1100));
    const r = await page.evaluate(() => ({
      title: [...document.querySelectorAll("main h1, main h2")].find((e) => e.offsetParent !== null)?.textContent ?? "",
      overflow: document.documentElement.scrollWidth > document.documentElement.clientWidth,
      text: document.querySelector("main")?.innerText.length ?? 0,
    }));
    const ok = r.title && !r.overflow && r.text > 40;
    if (!ok) failed++;
    console.log(`${ok ? "ok  " : "FAIL"} screen ${i + 1}/${ids}: ${r.title.slice(0, 50)}`);
  }
  // Komut paleti açılır.
  await page.keyboard.down("Control"); await page.keyboard.press("k"); await page.keyboard.up("Control");
  await new Promise((r) => setTimeout(r, 400));
  if (!(await page.$("dialog[open] .palette"))) { failed++; console.log("FAIL command palette"); } else console.log("ok   command palette");
  if (errors.length) { failed++; console.log("FAIL console errors:\n" + errors.join("\n")); }
} finally {
  await browser.close();
  server.kill();
}
process.exit(failed ? 1 : 0);
