// Renders the plugin UI (served by server.py) to PNGs with headless Chrome.
//   node shots.mjs [outdir]
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
async function open(w, h, dpr = 1) {
  const page = await browser.newPage();
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => { if (["error", "warning"].includes(m.type())) errors.push(`${m.type()}: ${m.text()}`); });
  await page.setViewport({ width: w, height: h, deviceScaleFactor: dpr });
  await page.goto(BASE + "/", { waitUntil: "load" });
  await page.evaluate(() => window.apf.ready);
  await page.evaluate(() => document.fonts.ready);
  await sleep(900);
  return page;
}
const shot = async (page, name) => { await page.screenshot({ path: `${OUT}/${name}.png` }); console.log("wrote", name); };
const center = async (page, sel) => { const b = await (await page.$(sel)).boundingBox(); return [b.x + b.width / 2, b.y + b.height / 2]; };

await state({ preset: "Classic Screw", playing: "1", level: "1" });
let page = await open(960, 600);
await shot(page, "01-default-960x600");

// hover tooltip while dragging the pitch knob
{
  const [x, y] = await center(page, ".knob.hero .k-face");
  await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x + 10, y + 40, { steps: 6 }); await sleep(250);
  await shot(page, "02-knob-drag-tooltip");
  await page.mouse.up();
}
// preset browser
{
  const btn = await page.$(".apf-presets .apf-name");
  if (btn) { await btn.click(); await sleep(400); await shot(page, "03-preset-browser"); await page.keyboard.press("Escape"); await page.mouse.click(5, 5); }
}
await page.close();

// chop editor with a preset that uses the gate
await state({ preset: "Trap Roll Chop" });
page = await open(960, 600);
await shot(page, "04-chop-pattern");
{
  // switch to Custom by drawing on the steps
  const cells = await page.$$(".step");
  const b0 = await cells[3].boundingBox(), b1 = await cells[9].boundingBox();
  await page.mouse.move(b0.x + 4, b0.y + b0.height / 2); await page.mouse.down(); await page.mouse.move(b1.x + 4, b1.y + b0.height / 2, { steps: 8 }); await page.mouse.up();
  await sleep(500);
  await shot(page, "05-chop-custom-drawn");
}
// A/B: set B to something different
{
  await page.click("#ab-b"); await sleep(200);
  const [x, y] = await center(page, "#g-space .knob:nth-child(2) .k-face");
  await page.mouse.move(x, y); await page.mouse.down(); await page.mouse.move(x, y - 60, { steps: 5 }); await page.mouse.up(); await sleep(500);
  await shot(page, "06-ab-slot-b");
  await page.keyboard.down("Control"); await page.keyboard.press("z"); await page.keyboard.up("Control"); await sleep(300);
  await shot(page, "07-after-undo");
}
// analyzer node hover
{
  const box = await (await page.$("#spec")).boundingBox();
  await page.mouse.move(box.x + box.width * 0.45, box.y + box.height * 0.55); await sleep(300);
  await shot(page, "08-analyzer-cursor-readout");
}
await page.close();

// bypassed, clipping, limiter working
await state({ preset: "Night Drive", level: "2.4" });
page = await open(960, 600);
await sleep(600);
await shot(page, "09-hot-signal-limiter");
await state({ bypass: "1", level: "1" });
await sleep(800);
await shot(page, "10-bypassed");
await state({ bypass: "0" });
await page.close();

// sizes and DPI
await state({ preset: "Slowed + Reverb" });
for (const [w, h, d, n] of [[760, 480, 1, "11-min-760x480"], [1440, 900, 1, "12-1440x900"], [1920, 1080, 1, "13-1920x1080"], [1200, 750, 2, "14-hidpi-1200x750@2x"], [2560, 1600, 1, "15-large-2560x1600"]]) {
  page = await open(w, h, d); await shot(page, n); await page.close();
}
fs.writeFileSync(`${OUT}/console-log.txt`, errors.join("\n") + (errors.length ? "\n" : "(no console errors or warnings)\n"));
console.log(errors.length ? errors.join("\n") : "no console errors");
await browser.close();
