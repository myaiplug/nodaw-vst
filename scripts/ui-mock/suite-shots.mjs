// Renders a suite plugin's UI (served by suite_server.py) to PNGs with headless Chrome.
//   node suite-shots.mjs <plugin> <outdir> [preset] [extra preset]
import puppeteer from "puppeteer-core";
import fs from "node:fs";
const [PLUGIN, OUT, PRESET = "Init", PRESET2 = ""] = process.argv.slice(2);
const [VW, VH] = (process.env.VIEW || "960x600").split("x").map(Number);
const BASE = process.env.MOCK || "http://127.0.0.1:8777";
const CHROME = process.env.CHROME || "/usr/bin/google-chrome";
fs.mkdirSync(OUT, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const state = (q) => fetch(`${BASE}/__mock/state?${new URLSearchParams(q)}`).then((r) => r.text());
const browser = await puppeteer.launch({ executablePath: CHROME, headless: "new", args: ["--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--font-render-hinting=none"] });
const errors = [];
async function open(w, h, dpr = 1) {
  const page = await browser.newPage();
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => { if (["error", "warning"].includes(m.type())) errors.push(`${m.type()}: ${m.text()}`); });
  await page.setViewport({ width: w, height: h, deviceScaleFactor: dpr });
  await page.goto(BASE + "/", { waitUntil: "load" });
  await page.evaluate(() => window.apf.ready);
  await page.evaluate(() => document.fonts.ready);
  await sleep(1200);
  return page;
}
const shot = async (page, name) => { await page.screenshot({ path: `${OUT}/${PLUGIN}-${name}.png` }); console.log("wrote", `${PLUGIN}-${name}`); };
const center = async (page, sel) => { const el = await page.$(sel); if (!el) return null; const b = await el.boundingBox(); return b && [b.x + b.width / 2, b.y + b.height / 2]; };

await state({ preset: PRESET, playing: "1", level: "1", license: "trial", days: "11" });
let page = await open(VW, VH);
await shot(page, `01-default-${VW}x${VH}`);
{
  const c = await center(page, ".knob .k-face");
  if (c) { const [x, y] = c; await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x + 8, y - 35, { steps: 6 }); await sleep(250); await shot(page, "02-knob-drag"); await page.mouse.up(); }
}
{
  const btn = await page.$(".apf-presets .apf-name");
  if (btn) { await btn.click(); await sleep(450); await shot(page, "03-preset-browser"); await page.keyboard.press("Escape"); await page.mouse.click(5, 5); }
}
// plugin-specific interactions
const hooks = {
  racks: async (p) => {
    const box = await (await p.$("#spec")).boundingBox();
    // drag band 5's node up
    await p.evaluate(() => document.querySelector('#eq-tabs button[data-i="4"]').click());
    await sleep(200);
    await p.mouse.move(box.x + box.width * 0.62, box.y + box.height * 0.5); await sleep(300);
    await shot(p, "04-eq-band-select");
    await p.click('#skins [data-skin="silver"]'); await sleep(400); await shot(p, "05-skin-silver");
    await p.click('#skins [data-skin="obsidian"]'); await sleep(400); await shot(p, "06-skin-obsidian");
    await p.click('#skins [data-skin="cobalt"]'); await sleep(300);
  },
  "harmonic-space": async (p) => {
    await p.click('#rc-fams [data-fam="Space"]'); await sleep(250);
    await state({ b2_solo: "1" }); await sleep(900);
    await p.evaluate(() => document.querySelector('#eq .band[data-i="1"]').dispatchEvent(new PointerEvent("pointerdown", { bubbles: true })));
    await sleep(300); await shot(p, "04-band-solo");
    await state({ b2_solo: "0" }); await sleep(300);
    await p.click('#looks [data-look="light"]'); await sleep(500); await shot(p, "05-light-faceplate");
    await p.click('#looks [data-look="dark"]'); await sleep(300);
  },
};
if (hooks[PLUGIN]) await hooks[PLUGIN](page);
await page.close();

if (PRESET2) {
  await state({ preset: PRESET2, level: "1.8" });
  page = await open(VW, VH);
  await sleep(500);
  await shot(page, "07-preset-" + PRESET2.replace(/[^A-Za-z0-9]+/g, "-").toLowerCase());
  await state({ bypass: "1", level: "1" }); await sleep(900); await shot(page, "08-bypassed"); await state({ bypass: "0" });
  await page.close();
}
await state({ license: "expired", days: "0" });
page = await open(VW, VH); await sleep(400); await shot(page, "09-trial-ended-dialog"); await page.close();
await state({ license: "trial", days: "11", preset: PRESET });
const [MW, MH] = (process.env.MIN || "760x480").split("x").map(Number);
for (const [w, h, d, n] of [[MW, MH, 1, `10-min-${MW}x${MH}`], [1440, 900, 1, "11-1440x900"], [1200, 750, 2, "12-hidpi-1200x750@2x"]]) {
  page = await open(w, h, d); await shot(page, n); await page.close();
}
fs.writeFileSync(`${OUT}/${PLUGIN}-console-log.txt`, errors.join("\n") + (errors.length ? "\n" : "(no console errors or warnings)\n"));
console.log(errors.length ? errors.join("\n") : "no console errors");
await browser.close();
