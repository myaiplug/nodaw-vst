// Screenshots of the trial badge and the activation dialog (served by server.py).
//   node trial-shots.mjs [outdir]
import puppeteer from "puppeteer-core";
import fs from "node:fs";
const OUT = process.argv[2] || "/workspace/apf-shots";
const BASE = process.env.MOCK || "http://127.0.0.1:8777";
const CHROME = process.env.CHROME || "/usr/bin/google-chrome";
fs.mkdirSync(OUT, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const state = (q) => fetch(`${BASE}/__mock/state?${new URLSearchParams(q)}`).then((r) => r.text());
const browser = await puppeteer.launch({ executablePath: CHROME, headless: "new", args: ["--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--font-render-hinting=none"] });
const errors = [];
async function open() {
  const page = await browser.newPage();
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  await page.setViewport({ width: 960, height: 600 });
  await page.goto(BASE + "/", { waitUntil: "load" });
  await page.evaluate(() => window.apf.ready);
  await page.evaluate(() => document.fonts.ready);
  await sleep(1000);
  return page;
}
const shot = async (page, name) => { await page.screenshot({ path: `${OUT}/${name}.png` }); console.log("wrote", name); };
await state({ preset: "Classic Screw", playing: "1", level: "1", license: "trial", days: "11" });
let page = await open();
await shot(page, "16-trial-badge");
await page.click("#lic-badge"); await sleep(400);
await shot(page, "17-trial-dialog");
await page.type("#lic-key", "1A2B3C4D-5E6F7A8B-9C0D1E2F-3A4B5C6D");
await page.click("#lic-activate"); await sleep(400);
await shot(page, "18-activation-refused");
await page.close();
await state({ license: "expired", days: "0" });
page = await open(); await sleep(300);
await shot(page, "19-trial-ended-dialog");
await page.click("#lic-close"); await sleep(250);
await shot(page, "20-trial-ended-badge");
await page.close();
await state({ license: "licensed", days: "0" });
page = await open();
await shot(page, "21-licensed-no-badge");
await page.evaluate(() => document.querySelector("#lic").hidden = false); await sleep(300);
await shot(page, "22-licensed-dialog");
await browser.close();
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
