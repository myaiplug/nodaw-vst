/* NoDAW UI kit: what every NoDAW plugin page shares (first written for NoDAW Screw).
 *
 *   NoDAW.shell({...})     header (presets, trial badge, A/B, undo/redo, bypass), footer and the
 *                          license dialog, built before apf.js binds the page
 *   NoDAW.knob(spec)       the SVG knob cell; NoDAW.knobs({containerId: [specs]})
 *   NoDAW.pill / choice    small switches and segmented choices bound to parameters
 *   NoDAW.start(opts)      after apf.ready: ranges, history (undo/redo of every edit), A/B,
 *                          presets in the history, the meter frame, the spectrum, meters,
 *                          footer, trial/license; then opts.init(ctx) for the plugin's own parts
 *
 * Every control is a host parameter (apf.begin/set/end), so hosts see the gestures:
 * automation, host undo, MIDI learn. Session state that is not a parameter goes through the
 * plugin's controller ("x" in the saved state) via ctx.ext = {get, set, refresh}.
 */
(() => {
  "use strict";
  const K = (window.NoDAW = window.NoDAW || {});

  /* ── helpers ─────────────────────────────────────────────────────────────────────── */
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const clamp01 = (v) => clamp(v, 0, 1);
  const lerp = (a, b, t) => a + (b - a) * t;
  const NS = "http://www.w3.org/2000/svg";
  const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const MINUS = "\u2212";
  const sign = (s) => String(s).replace("-", MINUS);
  const lin2db = (v) => (v > 1e-6 ? 20 * Math.log10(v) : -120);
  const rgb = (name, a) => `rgba(${css(name) || "168,85,247"},${a})`;
  Object.assign(K, { $, $$, clamp, clamp01, lerp, rem, css, sign, MINUS, lin2db, rgb });

  K.fitRoot = (w = 960, h = 600) => {
    const fit = () => { const u = Math.min(innerWidth / w, innerHeight / h); document.documentElement.style.fontSize = `${13 * clamp(u, 0.74, 2.8)}px`; };
    fit(); addEventListener("resize", fit);
  };

  function svg(tag, attrs, parent) {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  }
  K.svg = svg;
  function h(tag, cls, html) { const e = document.createElement(tag); if (cls) e.className = cls; if (html != null) e.innerHTML = html; return e; }
  K.h = h;

  /* ── the shell: header, footer, tooltip, license dialog ──────────────────────────── */
  const ICON = {
    copy: '<svg viewBox="0 0 16 16"><path d="M3 8h9M9 4.5 12.5 8 9 11.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    undo: '<svg viewBox="0 0 16 16"><path d="M5.5 3.5 2.5 6.5l3 3M3 6.5h6.5a3.5 3.5 0 0 1 0 7H7" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    redo: '<svg viewBox="0 0 16 16"><path d="m10.5 3.5 3 3-3 3M13 6.5H6.5a3.5 3.5 0 0 0 0 7H9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    power: '<svg viewBox="0 0 16 16"><path d="M8 1.8v5.6M4.6 4a5.2 5.2 0 1 0 6.8 0" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg>',
    x: '<svg viewBox="0 0 16 16"><path d="m4 4 8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>',
  };
  K.ICON = ICON;
  let SHELL = { name: "NoDAW", prod: "", hint: "" };

  K.shell = (o) => {
    SHELL = Object.assign({ hint: "Drag to adjust · Shift for fine · Double-click to reset · Right-click for MIDI learn", version: "v0.1", bypassKey: "bypass" }, o);
    const app = $("#app");
    const top = h("header", "top");
    top.innerHTML = `
      <div class="brand" title="${SHELL.name}"><span class="logo">NODAW</span><span class="prod">${SHELL.prod}</span></div>
      <div class="presets" data-apf-presets></div>
      <div class="tools">
        <button type="button" id="lic-badge" class="lic-badge" hidden title="License and trial"><i class="lic-dot"></i><span class="lic-txt">Trial</span></button>
        <div class="seg" role="group" aria-label="A/B compare">
          <button type="button" id="ab-a" class="on" title="Compare slot A">A</button>
          <button type="button" id="ab-b" title="Compare slot B">B</button>
          <button type="button" id="ab-copy" class="ico" title="Copy the current settings to the other slot" aria-label="Copy to the other slot">${ICON.copy}</button>
        </div>
        <div class="seg" role="group" aria-label="History">
          <button type="button" id="undo" class="ico" title="Undo (Ctrl+Z)" aria-label="Undo" disabled>${ICON.undo}</button>
          <button type="button" id="redo" class="ico" title="Redo (Ctrl+Shift+Z)" aria-label="Redo" disabled>${ICON.redo}</button>
        </div>
        <label class="power" title="Bypass (latency stays compensated)">
          <input type="checkbox" data-param="${SHELL.bypassKey}" aria-label="Bypass">
          <span class="power-ico">${ICON.power}</span><span class="power-txt">Bypass</span>
        </label>
        <button type="button" class="ico solo" data-apf-maximize></button>
      </div>`;
    app.prepend(top);
    const foot = h("footer", "foot");
    foot.innerHTML = `
      <span class="f-item" id="f-lat" title="Reported to the host for delay compensation">Latency —</span><span class="f-sep"></span>
      <span class="f-item" id="f-sr">— kHz</span><span class="f-sep"></span>
      <span class="f-item" id="f-tr"><i class="dot"></i><b>Stopped</b></span>
      <span class="f-hint" id="f-hint">${SHELL.hint}</span>
      <span class="f-ver">NoDAW · ${SHELL.version}</span>`;
    app.append(foot);
    const grip = h("div"); grip.setAttribute("data-apf-resize-grip", ""); app.append(grip);
    const tip = h("div", "tip"); tip.id = "tip"; tip.setAttribute("role", "status"); document.body.append(tip);
    const lic = h("div", "lic");
    lic.id = "lic"; lic.hidden = true; lic.setAttribute("role", "dialog"); lic.setAttribute("aria-modal", "true"); lic.setAttribute("aria-labelledby", "lic-title");
    lic.innerHTML = `
      <div class="lic-card">
        <button type="button" class="lic-x" id="lic-close" aria-label="Close">${ICON.x}</button>
        <div class="lic-head"><span class="logo">NODAW</span><span class="prod">${SHELL.prod}</span></div>
        <h2 id="lic-title" class="lic-title">Your trial</h2>
        <div class="lic-meter" aria-hidden="true"><i id="lic-bar"></i></div>
        <p class="lic-sub" id="lic-sub"></p>
        <form class="lic-form" id="lic-form" autocomplete="off">
          <label class="lic-label" for="lic-key">License key</label>
          <div class="lic-row">
            <input id="lic-key" type="text" spellcheck="false" placeholder="XXXXXXXX-XXXXXXXX-XXXXXXXX-XXXXXXXX" aria-describedby="lic-msg">
            <button type="submit" class="lic-btn lic-primary" id="lic-activate">Activate</button>
          </div>
          <p class="lic-msg" id="lic-msg" role="status" aria-live="polite"></p>
        </form>
        <div class="lic-licensed" id="lic-licensed" hidden><p><b id="lic-who"></b></p><p class="lic-fine" id="lic-keyhint"></p></div>
        <div class="lic-foot">
          <button type="button" class="lic-btn lic-buy" id="lic-buy">Buy ${SHELL.name}</button>
          <span class="lic-fine" id="lic-fine">Your key is in the Gumroad receipt email.</span>
        </div>
      </div>`;
    document.body.append(lic);
  };

  const hint = (t) => { const e = $("#f-hint"); if (e) e.textContent = t || SHELL.hint; };
  K.hint = hint;

  /* ── SVG knob ────────────────────────────────────────────────────────────────────── */
  const A0 = 135, SWEEP = 270;
  const pt = (r, deg) => [50 + r * Math.cos((deg * Math.PI) / 180), 50 + r * Math.sin((deg * Math.PI) / 180)];
  function arc(r, a0, a1) {
    if (Math.abs(a1 - a0) < 0.05) return "";
    const [x0, y0] = pt(r, a0), [x1, y1] = pt(r, a1);
    return `M${x0.toFixed(2)} ${y0.toFixed(2)}A${r} ${r} 0 ${Math.abs(a1 - a0) > 180 ? 1 : 0} ${a1 > a0 ? 1 : 0} ${x1.toFixed(2)} ${y1.toFixed(2)}`;
  }
  let defsDone = false;
  function svgDefs() {
    if (defsDone) return; defsDone = true;
    const s = svg("svg", { width: 0, height: 0, "aria-hidden": "true", style: "position:absolute" }, document.body);
    const d = svg("defs", {}, s);
    const lg = svg("linearGradient", { id: "kgrad", x1: 0, y1: 1, x2: 1, y2: 0 }, d);
    svg("stop", { offset: 0, "stop-color": css("--accent-2") }, lg);
    svg("stop", { offset: 0.55, "stop-color": css("--accent") }, lg);
    svg("stop", { offset: 1, "stop-color": css("--accent-hot") }, lg);
    const body = svg("radialGradient", { id: "kbody", cx: 0.5, cy: 0.32, r: 0.8 }, d);
    svg("stop", { offset: 0, "stop-color": css("--knob-hi") || "#4a4a58" }, body);
    svg("stop", { offset: 0.55, "stop-color": css("--knob-mid") || "#1f1f27" }, body);
    svg("stop", { offset: 1, "stop-color": css("--knob-lo") || "#0d0d12" }, body);
    const rim = svg("linearGradient", { id: "krim", x1: 0, y1: 0, x2: 0, y2: 1 }, d);
    svg("stop", { offset: 0, "stop-color": "rgba(255,255,255,.55)" }, rim);
    svg("stop", { offset: 0.45, "stop-color": "rgba(255,255,255,.05)" }, rim);
    svg("stop", { offset: 1, "stop-color": "rgba(255,255,255,.18)" }, rim);
    const cap = svg("radialGradient", { id: "kcap", cx: 0.4, cy: 0.3, r: 0.9 }, d);
    svg("stop", { offset: 0, "stop-color": css("--knob-cap-hi") || "#2e2e39" }, cap);
    svg("stop", { offset: 1, "stop-color": css("--knob-cap-lo") || "#121217" }, cap);
    const sh = svg("radialGradient", { id: "kshadow" }, d);
    svg("stop", { offset: 0.7, "stop-color": "rgba(0,0,0,.65)" }, sh);
    svg("stop", { offset: 1, "stop-color": "rgba(0,0,0,0)" }, sh);
  }
  K.refreshDefs = () => { const old = $("#kgrad"); if (old) old.closest("svg").remove(); defsDone = false; svgDefs(); };

  /** spec: { k: param key, hero, mini, tiny, bipolar, hint, label (override the name) } */
  K.knob = (spec) => {
    svgDefs();
    const cell = h("div", `knob${spec.hero ? " hero" : ""}${spec.mini ? " mini" : ""}${spec.tiny ? " tiny" : ""}`);
    cell.dataset.key = spec.k;
    cell.dataset.hint = spec.hint || "";
    if (spec.bipolar) cell.dataset.bipolar = "1";
    if (spec.color) cell.style.setProperty("--kc", spec.color);
    cell.setAttribute("data-param-learn", spec.k);
    const face = h("div", "k-face");
    face.tabIndex = 0; face.setAttribute("role", "slider"); face.setAttribute("aria-valuemin", "0"); face.setAttribute("aria-valuemax", "1");
    const s = svg("svg", { viewBox: "0 0 100 100" }, face);
    for (let i = 0; i <= 10; i++) {
      const a = A0 + (SWEEP * i) / 10;
      const [x0, y0] = pt(i % 5 === 0 ? 44.5 : 45.5, a), [x1, y1] = pt(48.5, a);
      svg("line", { class: `k-tick${i % 5 === 0 ? " maj" : ""}`, x1: x0, y1: y0, x2: x1, y2: y1, "stroke-width": 1.4, "stroke-linecap": "round" }, s);
    }
    svg("path", { class: "k-track", d: arc(40, A0, A0 + SWEEP), fill: "none", "stroke-width": 5, "stroke-linecap": "round" }, s);
    svg("path", { class: "k-ring", d: "", fill: "none", "stroke-width": 5, "stroke-linecap": "round" }, s);
    svg("circle", { cx: 50, cy: 54, r: 36, fill: "url(#kshadow)" }, s);
    svg("circle", { cx: 50, cy: 50, r: 31, fill: "url(#kbody)", stroke: "#000", "stroke-width": 1 }, s);
    svg("circle", { cx: 50, cy: 50, r: 30, fill: "none", stroke: "url(#krim)", "stroke-width": 1.1 }, s);
    svg("circle", { cx: 50, cy: 50, r: 22.5, fill: "url(#kcap)", stroke: "rgba(0,0,0,.7)", "stroke-width": 0.8 }, s);
    svg("circle", { cx: 50, cy: 50, r: 21.7, fill: "none", stroke: "rgba(255,255,255,.07)", "stroke-width": 0.8 }, s);
    svg("line", { class: "k-ptr", x1: 50, y1: 50, x2: 50, y2: 50, "stroke-width": 3.6, "stroke-linecap": "round" }, s);
    const label = h("div", "k-label");
    if (spec.label) label.textContent = spec.label; else label.setAttribute("data-param-name", spec.k);
    const value = h("div", "k-value");
    value.setAttribute("data-param-text", spec.k);
    value.title = "Click to type a value";
    cell.append(face, label, value);
    return cell;
  };
  K.knobs = (map) => { for (const [id, list] of Object.entries(map)) { const host = document.getElementById(id); for (const spec of list) host.appendChild(K.knob(spec)); } };

  /** A small toggle bound to a bool parameter. */
  K.pill = (key, text, cls = "") => {
    const l = h("label", `pill ${cls}`);
    l.innerHTML = `<input type="checkbox" data-param="${key}" aria-label="${text}"><span>${text}</span>`;
    l.dataset.key = key;
    return l;
  };
  /** A switch (bigger, labelled) bound to a bool parameter. */
  K.switch = (key, text) => {
    const l = h("label", "switch");
    l.innerHTML = `<input type="checkbox" data-param="${key}" aria-label="${text}"><span class="sw"><i></i></span><span>${text}</span>`;
    return l;
  };
  /** A segmented choice for a choice parameter; filled in start(). */
  K.choice = (key, labels) => {
    const d = h("div", "choice");
    d.dataset.choice = key;
    if (labels) d.dataset.labels = labels.join("|");
    d.setAttribute("data-param-learn", key);
    return d;
  };
  /** A labelled <select> bound to a choice parameter. */
  K.select = (key, cls = "") => {
    const l = h("label", `sel ${cls}`);
    l.innerHTML = `<span data-param-name="${key}"></span><select data-param="${key}"></select>`;
    return l;
  };

  /* ── biquad magnitude (RBJ forms, the same as crates/nodaw-dsp/src/eq.rs) ────────── */
  const BQ = (K.bq = {});
  const nrm = (b0, b1, b2, a0, a1, a2) => [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0];
  const om = (fc, sr) => { const w = (2 * Math.PI * clamp(fc, 1, sr * 0.49)) / sr; return [Math.sin(w), Math.cos(w)]; };
  BQ.peak = (fc, q, g, sr) => { const A = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(1 + al * A, -2 * c, 1 - al * A, 1 + al / A, -2 * c, 1 - al / A); };
  BQ.lowShelf = (fc, q, g, sr) => { const A = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)), sq = 2 * Math.sqrt(A) * al; return nrm(A * (A + 1 - (A - 1) * c + sq), 2 * A * (A - 1 - (A + 1) * c), A * (A + 1 - (A - 1) * c - sq), A + 1 + (A - 1) * c + sq, -2 * (A - 1 + (A + 1) * c), A + 1 + (A - 1) * c - sq); };
  BQ.highShelf = (fc, q, g, sr) => { const A = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)), sq = 2 * Math.sqrt(A) * al; return nrm(A * (A + 1 + (A - 1) * c + sq), -2 * A * (A - 1 + (A + 1) * c), A * (A + 1 + (A - 1) * c - sq), A + 1 - (A - 1) * c + sq, 2 * (A - 1 - (A + 1) * c), A + 1 - (A - 1) * c - sq); };
  BQ.highpass = (fc, q, sr) => { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm((1 + c) / 2, -(1 + c), (1 + c) / 2, 1 + al, -2 * c, 1 - al); };
  BQ.lowpass = (fc, q, sr) => { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm((1 - c) / 2, 1 - c, (1 - c) / 2, 1 + al, -2 * c, 1 - al); };
  BQ.notch = (fc, q, sr) => { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(1, -2 * c, 1, 1 + al, -2 * c, 1 - al); };
  BQ.bandpass = (fc, q, sr) => { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(al, 0, -al, 1 + al, -2 * c, 1 - al); };
  BQ.db = (c, f, sr) => {
    const w = (2 * Math.PI * f) / sr, c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
    const nr = c[0] + c[1] * c1 + c[2] * c2, ni = -(c[1] * s1 + c[2] * s2), dr = 1 + c[3] * c1 + c[4] * c2, di = -(c[3] * s1 + c[4] * s2);
    return 10 * Math.log10(Math.max(1e-12, (nr * nr + ni * ni) / (dr * dr + di * di)));
  };
  /** shape: "peak" | "lowShelf" | "highShelf" | "highpass" | "lowpass" | "notch" | "bandpass" */
  BQ.shapeDb = (shape, fc, q, g, f, sr) => {
    if ((shape === "peak" || shape === "lowShelf" || shape === "highShelf") && Math.abs(g) < 1e-3) return 0;
    const c = shape === "highpass" || shape === "lowpass" || shape === "notch" || shape === "bandpass" ? BQ[shape](fc, q, sr) : BQ[shape](fc, q, g, sr);
    return BQ.db(c, f, sr);
  };

  K.sizeCanvas = (c, ctx2, box) => {
    const dpr = window.devicePixelRatio || 1, r = (box || c).getBoundingClientRect();
    const w = Math.max(2, Math.round(r.width * dpr)), hh = Math.max(2, Math.round(r.height * dpr));
    if (c.width !== w || c.height !== hh) { c.width = w; c.height = hh; }
    ctx2.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { w: r.width, h: r.height, dpr };
  };

  /* ── start: everything that needs the parameter table ────────────────────────────── */
  K.start = (opts = {}) => apf.ready.then(async () => {
    const app = $("#app");
    const params = apf.params;
    const keyIndex = Object.fromEntries(params.map((p, i) => [p.key, i]));
    const P = (k) => apf.param(k);
    const bypassKey = SHELL.bypassKey || "bypass";

    let R = {};
    try { R = await apf.plugin.get("ranges"); } catch (e) { apf.log("ranges:", String(e)); }
    const plainOf = (k, n) => {
      const r = R[k]; if (!r) { const p = P(k); return p && p.steps ? Math.round(n * p.steps) : n; } n = clamp01(n);
      if (r.curve === "log") return r.min * Math.pow(r.max / r.min, n);
      if (r.curve === "pow") return r.min + (r.max - r.min) * Math.pow(n, r.exp);
      return r.min + (r.max - r.min) * n;
    };
    const normOf = (k, v) => {
      const r = R[k]; if (!r) { const p = P(k); return p && p.steps ? clamp01(v / p.steps) : clamp01(v); } if (r.max === r.min) return 0;
      v = clamp(v, Math.min(r.min, r.max), Math.max(r.min, r.max));
      if (r.curve === "log") return clamp01(Math.log(v / r.min) / Math.log(r.max / r.min));
      if (r.curve === "pow") return clamp01(Math.pow((v - r.min) / (r.max - r.min), 1 / r.exp));
      return clamp01((v - r.min) / (r.max - r.min));
    };
    const plainNow = (k) => plainOf(k, P(k).value);
    const on = (k) => P(k) && P(k).value >= 0.5;
    const idx = (k) => { const p = P(k); return p ? Math.round(p.value * (p.steps || 1)) : 0; };
    function parseTyped(k, text) {
      const r = R[k]; if (!r) return null;
      let t = text.trim().toLowerCase().replace(MINUS, "-");
      let scale = 1;
      const unit = (r.unit || "").toLowerCase();
      if (unit === "hz" && t.endsWith("khz")) { t = t.slice(0, -3); scale = 1000; }
      else if (unit && t.endsWith(unit)) t = t.slice(0, -unit.length);
      t = t.trim();
      if (unit === "hz" && t.endsWith("k")) { t = t.slice(0, -1).trim(); scale = 1000; }
      const v = Number(t);
      return Number.isFinite(v) && t !== "" ? normOf(k, v * scale) : null;
    }

    const ctx = { params, P, R, plainOf, normOf, plainNow, on, idx, parseTyped, hint, app, ext: null };

    /* history */
    const H = { u: [], r: [] };
    let replaying = false;
    const undoBtn = $("#undo"), redoBtn = $("#redo");
    const syncHist = () => { undoBtn.disabled = !H.u.length; redoBtn.disabled = !H.r.length; };
    let committed = null;
    const snapshot = () => {
      const p = {};
      for (const d of params) if (d.key !== bypassKey) p[d.key] = P(d.key).value;
      return { p, s: ctx.ext ? ctx.ext.get() : null };
    };
    function applySnap(sn) {
      for (const [k, v] of Object.entries(sn.p)) {
        if (!(k in keyIndex)) continue;
        if (Math.abs(P(k).value - v) > 1e-9) apf.change(k, v);
      }
      if (ctx.ext && sn.s != null && JSON.stringify(sn.s) !== JSON.stringify(ctx.ext.get())) ctx.ext.set(sn.s);
    }
    function push(entry) {
      if (replaying) return;
      H.u.push(entry); if (H.u.length > 200) H.u.shift();
      H.r.length = 0; committed = snapshot(); syncHist(); scheduleAb();
    }
    function replay(fn) { replaying = true; try { fn(); } finally { replaying = false; } committed = snapshot(); syncHist(); scheduleAb(); }
    function undo() { const e = H.u.pop(); if (!e) return; replay(e.undo); H.r.push(e); syncHist(); }
    function redo() { const e = H.r.pop(); if (!e) return; replay(e.redo); H.u.push(e); syncHist(); }
    undoBtn.addEventListener("click", undo);
    redoBtn.addEventListener("click", redo);
    addEventListener("keydown", (e) => {
      if (e.target && e.target.tagName === "INPUT" && e.target.type === "text") return;
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) { e.preventDefault(); undo(); }
      else if ((k === "z" && e.shiftKey) || k === "y") { e.preventDefault(); redo(); }
    });
    apf.keys.claim(["cmd+z", "cmd+shift+z", "cmd+y"]);
    const recordParam = (key, from, to) => { if (Math.abs(from - to) < 1e-9) return; push({ undo: () => apf.change(key, from), redo: () => apf.change(key, to) }); };
    /** Several parameters changed by one gesture: one history entry. */
    const recordMany = (from, to) => {
      const keys = Object.keys(to).filter((k) => Math.abs((from[k] ?? to[k]) - to[k]) > 1e-9);
      if (!keys.length) return;
      push({ undo: () => keys.forEach((k) => apf.change(k, from[k])), redo: () => keys.forEach((k) => apf.change(k, to[k])) });
    };
    /** Set several parameters (plain values) as one edit with one history entry. */
    const changeMany = (plainMap) => {
      const from = {}, to = {};
      for (const [k, v] of Object.entries(plainMap)) { if (!(k in keyIndex)) continue; from[k] = P(k).value; to[k] = typeof v === "boolean" ? (v ? 1 : 0) : normOf(k, v); }
      for (const k of Object.keys(to)) apf.change(k, to[k]);
      recordMany(from, to);
    };
    /** Session (non-parameter) edit with undo. */
    const recordExt = (from, to) => { if (JSON.stringify(from) === JSON.stringify(to)) return; push({ undo: () => ctx.ext.set(from), redo: () => ctx.ext.set(to) }); };
    Object.assign(ctx, { snapshot, applySnap, push, replay, undo, redo, recordParam, recordMany, changeMany, recordExt, history: H });

    /* knobs */
    const tip = $("#tip");
    let tipTimer = 0;
    const updateTip = (key) => { const v = $(".t-v", tip); if (v) v.textContent = sign(P(key).text || ""); };
    function showTip(cell, key) {
      const r = ($(".k-face", cell) || cell).getBoundingClientRect();
      tip.innerHTML = `<span class="t-n"></span><span class="t-v"></span>`;
      tip.firstChild.textContent = P(key).name;
      tip.style.left = `${r.left + r.width / 2}px`; tip.style.top = `${r.top}px`;
      updateTip(key); tip.classList.add("show"); clearTimeout(tipTimer);
    }
    const hideTip = (delay = 0) => { clearTimeout(tipTimer); tipTimer = setTimeout(() => tip.classList.remove("show"), delay); };
    Object.assign(ctx, { showTip, hideTip, updateTip });

    const knobs = {};
    function attachKnob(cell) {
      if (cell.dataset.attached) return; cell.dataset.attached = "1";
      const key = cell.dataset.key;
      const p = P(key);
      if (!p) return;
      const face = $(".k-face", cell), ring = $(".k-ring", cell), ptr = $(".k-ptr", cell), valueBox = $(".k-value", cell);
      const bipolar = cell.dataset.bipolar && R[key] ? normOf(key, cell.dataset.center ? Number(cell.dataset.center) : 0) : null;
      face.setAttribute("aria-label", p.name);
      cell.addEventListener("pointerenter", () => hint(cell.dataset.hint));
      cell.addEventListener("pointerleave", () => hint(""));
      const draw = (n) => {
        const a = A0 + SWEEP * n;
        ring.setAttribute("d", bipolar !== null ? arc(40, A0 + SWEEP * bipolar, a) : arc(40, A0, a));
        const [x0, y0] = pt(9, a), [x1, y1] = pt(28.5, a);
        ptr.setAttribute("x1", x0); ptr.setAttribute("y1", y0); ptr.setAttribute("x2", x1); ptr.setAttribute("y2", y1);
        face.setAttribute("aria-valuenow", n.toFixed(4));
      };
      apf.on(key, (v, text) => { draw(v); face.setAttribute("aria-valuetext", text); if (cell.classList.contains("active")) updateTip(key); });
      knobs[key] = { cell, draw };
      let drag = null;
      const detent = (v, e) => {
        if (bipolar !== null && Math.abs(v - bipolar) < 0.006 && !e.shiftKey) return bipolar;
        if (p.steps) return Math.round(v * p.steps) / p.steps;
        if (cell.dataset.snap && (e.altKey || e.ctrlKey)) return normOf(key, Math.round(plainOf(key, v)));
        return v;
      };
      face.addEventListener("pointerdown", (e) => {
        if (e.button !== 0 || (e.ctrlKey && e.pointerType === "mouse" && navigator.platform.startsWith("Mac"))) return;
        face.setPointerCapture(e.pointerId);
        drag = { x: e.clientX, y: e.clientY, v: p.value, from: p.value };
        apf.begin(key); cell.classList.add("active"); showTip(cell, key); face.focus({ preventScroll: true });
        e.preventDefault();
      });
      face.addEventListener("pointermove", (e) => {
        if (!drag) return;
        const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
        drag.x = e.clientX; drag.y = e.clientY;
        const per = (e.shiftKey ? 1 / 2100 : 1 / 260) * (cell.classList.contains("hero") ? 0.8 : 1);
        drag.v = clamp01(drag.v + (dx - dy) * per);
        apf.set(key, detent(drag.v, e));
      });
      const finish = () => { if (!drag) return; apf.end(key); cell.classList.remove("active"); hideTip(450); recordParam(key, drag.from, p.value); drag = null; };
      face.addEventListener("pointerup", finish);
      face.addEventListener("pointercancel", finish);
      face.addEventListener("lostpointercapture", finish);
      face.addEventListener("dblclick", () => { const from = p.value; apf.change(key, p.default); recordParam(key, from, p.default); });
      face.addEventListener("contextmenu", (e) => { if (drag) e.preventDefault(); });
      let wheel = null;
      face.addEventListener("wheel", (e) => {
        e.preventDefault();
        if (!wheel) { wheel = { from: p.value, v: p.value, t: 0 }; apf.begin(key); }
        const step = p.steps ? 1 / p.steps : e.shiftKey ? 0.002 : 0.012;
        wheel.v = clamp01(wheel.v + (e.deltaY < 0 ? 1 : -1) * step);
        apf.set(key, detent(wheel.v, e));
        cell.classList.add("active"); showTip(cell, key);
        clearTimeout(wheel.t);
        wheel.t = setTimeout(() => { apf.end(key); cell.classList.remove("active"); hideTip(250); recordParam(key, wheel.from, p.value); wheel = null; }, 280);
      }, { passive: false });
      face.addEventListener("keydown", (e) => {
        const big = e.key === "PageUp" || e.key === "PageDown";
        const dir = e.key === "ArrowUp" || e.key === "ArrowRight" || e.key === "PageUp" ? 1 : e.key === "ArrowDown" || e.key === "ArrowLeft" || e.key === "PageDown" ? -1 : 0;
        let v = null;
        if (dir) v = clamp01(p.value + dir * (p.steps ? 1 / p.steps : big ? 0.1 : e.shiftKey ? 0.002 : 0.01));
        else if (e.key === "Home") v = 0;
        else if (e.key === "End") v = 1;
        else if (e.key === "Enter") { beginEdit(); e.preventDefault(); return; }
        if (v === null) return;
        e.preventDefault();
        const from = p.value; apf.change(key, v); recordParam(key, from, v);
      });
      function beginEdit() {
        if ($("input", valueBox)) return;
        const input = document.createElement("input");
        input.type = "text"; input.value = sign(p.text || ""); input.setAttribute("aria-label", `${p.name} value`);
        const keep = valueBox.textContent;
        valueBox.removeAttribute("data-param-text");
        valueBox.textContent = ""; valueBox.appendChild(input);
        input.focus(); input.select();
        let done = false;
        const end = (commit) => {
          if (done) return; done = true;
          valueBox.setAttribute("data-param-text", key); valueBox.textContent = p.text || keep;
          if (commit) { const n = parseTyped(key, input.value); if (n !== null) { const from = p.value; apf.change(key, n); recordParam(key, from, n); } }
          face.focus({ preventScroll: true });
        };
        input.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Enter") end(true); else if (e.key === "Escape") end(false); });
        input.addEventListener("blur", () => end(true));
      }
      valueBox.addEventListener("click", beginEdit);
    }
    ctx.attachKnob = attachKnob;
    ctx.knobs = knobs;
    ctx.bind = (root) => { apf.bind(root); $$(".knob", root).forEach(attachKnob); bindChoices(root); bindHistory(root); };

    /* selects, switches, choices: history + hints */
    const cur = {}, prev = {};
    for (const d of params) { cur[d.key] = prev[d.key] = d.value; apf.on(d.key, (v) => { if (v !== cur[d.key]) { prev[d.key] = cur[d.key]; cur[d.key] = v; } }); }
    function bindHistory(root) {
      $$("select[data-param], input[type=checkbox][data-param]", root).forEach((input) => {
        const key = input.dataset.param;
        if (key === bypassKey || input.dataset.hist) return;
        input.dataset.hist = "1";
        input.addEventListener("change", () => recordParam(key, prev[key], cur[key]));
      });
    }
    function bindChoices(root) {
      $$("[data-choice]", root).forEach((box) => {
        if (box.dataset.bound) return; box.dataset.bound = "1";
        const key = box.dataset.choice, p = P(key); if (!p) return;
        const labels = box.dataset.labels ? box.dataset.labels.split("|") : p.options || [];
        const btns = labels.map((t, i) => {
          const b = h("button", "", t); b.type = "button"; b.title = (p.options && p.options[i]) || t;
          b.addEventListener("click", () => { const from = p.value, to = p.steps ? i / p.steps : 0; apf.change(key, to); recordParam(key, from, to); });
          box.appendChild(b); return b;
        });
        apf.on(key, (v) => { const i = Math.round(v * (p.steps || 1)); btns.forEach((b, j) => b.classList.toggle("on", j === i)); });
      });
    }
    $$(".knob").forEach(attachKnob);
    bindChoices(document);
    bindHistory(document);
    apf.on(bypassKey, (v) => { app.classList.toggle("bypassed", v >= 0.5); });

    /* the frame: meters + spectra (+ the plugin's extra floats) */
    const BANDS = 160, HEAD = 16;
    const meters = { sr: 0, lat: 0, playing: 0, inPk: [0, 0], outPk: [0, 0], inRms: [0, 0], outRms: [0, 0], gr: 1, custom: [0, 0, 0, 0], lastData: 0 };
    const bandsIn = new Float32Array(BANDS).fill(-120), bandsOut = new Float32Array(BANDS).fill(-120);
    let extra = new Float32Array(0);
    const frameFns = [];
    function ingest(buf) {
      if (!buf || buf.byteLength < (HEAD + 2 * BANDS) * 4) return false;
      const f = new Float32Array(buf);
      meters.inPk[0] = f[0]; meters.inPk[1] = f[1]; meters.outPk[0] = f[2]; meters.outPk[1] = f[3];
      meters.inRms[0] = f[4]; meters.inRms[1] = f[5]; meters.outRms[0] = f[6]; meters.outRms[1] = f[7];
      meters.gr = f[8] > 0 ? f[8] : 1; meters.sr = f[10]; meters.lat = f[11]; meters.playing = f[12];
      meters.custom = [f[9], f[13], f[14], f[15]];
      bandsIn.set(f.subarray(HEAD, HEAD + BANDS)); bandsOut.set(f.subarray(HEAD + BANDS, HEAD + 2 * BANDS));
      extra = f.slice(HEAD + 2 * BANDS); ctx.extra = extra;
      meters.lastData = performance.now();
      for (const fn of frameFns) fn(extra, meters);
      return f[0] > 1e-5 || f[1] > 1e-5 || f[2] > 1e-5 || f[3] > 1e-5 || f[12] > 0;
    }
    Object.assign(ctx, { meters, bandsIn, bandsOut, extra, onFrame: (fn) => frameFns.push(fn), BANDS });
    ctx.stale = () => performance.now() - meters.lastData > 700;

    /* the spectrum display (input, output, peak hold, the plugin's response curve and nodes) */
    ctx.spectrum = (canvas, cfg = {}) => {
      const FLO = 20, FHI = 20000, LOGSPAN = Math.log10(FHI / FLO);
      const bandHz = (i) => FLO * Math.pow(FHI / FLO, (i + 0.5) / BANDS);
      const showIn = new Float32Array(BANDS).fill(-120), showOut = new Float32Array(BANDS).fill(-120), hold = new Float32Array(BANDS).fill(-120), holdAge = new Float32Array(BANDS);
      const sctx = canvas.getContext("2d");
      const L = { w: 1, h: 1, l: 0, r: 0, t: 0, b: 0, pw: 1, ph: 1 };
      const DB_TOP = cfg.dbTop ?? 6, DB_BOT = cfg.dbBot ?? -90, EQ = cfg.eqRange ?? 18;
      let tilt = cfg.tilt ?? 3;
      let dirty = true;
      new ResizeObserver(() => { dirty = true; }).observe(canvas.parentElement);
      const fx = (f) => L.l + (Math.log10(f / FLO) / LOGSPAN) * L.pw;
      const fOfX = (x) => FLO * Math.pow(10, ((x - L.l) / L.pw) * LOGSPAN);
      const dy = (db) => L.t + ((DB_TOP - db) / (DB_TOP - DB_BOT)) * L.ph;
      const ey = (db) => L.t + L.ph * (0.5 - db / (2 * EQ));
      const dbOfEy = (y) => (0.5 - (y - L.t) / L.ph) * 2 * EQ;
      const tiltOf = (f) => tilt * Math.log2(f / 1000);
      const mouse = { x: -1, y: -1, in: false };
      let drag = null, hover = null;
      const S = { L, fx, fOfX, dy, ey, dbOfEy, mouse, setTilt: (t) => { tilt = t; hold.fill(-120); }, get tilt() { return tilt; }, selected: null };
      const nodes = () => (cfg.nodes ? cfg.nodes() : []);
      function nodeXY(n) { return { x: fx(clamp(n.f, FLO, FHI)), y: ey(clamp(n.db ?? 0, -EQ, EQ)) }; }
      function hit(x, y) {
        const r = 0.9 * rem(); let best = null, bd = r;
        for (const n of nodes()) { if (n.hidden) continue; const p = nodeXY(n), d = Math.hypot(p.x - x, p.y - y); if (d < bd) { bd = d; best = n; } }
        return best;
      }
      function draw(dt) {
        const c = sctx, R2 = rem();
        if (dirty) { const s = K.sizeCanvas(canvas, c); L.w = s.w; L.h = s.h; L.l = 2.7 * R2; L.r = (cfg.eqAxis === false ? 0.9 : 2.7) * R2; L.t = (cfg.top ?? 2.1) * R2; L.b = 1.5 * R2; L.pw = Math.max(10, L.w - L.l - L.r); L.ph = Math.max(10, L.h - L.t - L.b); dirty = false; }
        c.clearRect(0, 0, L.w, L.h);
        const bg = c.createLinearGradient(0, L.t, 0, L.t + L.ph);
        bg.addColorStop(0, css("--plate-1") || "#0b0a12"); bg.addColorStop(1, css("--plate-2") || "#060609");
        c.fillStyle = bg; c.fillRect(L.l, L.t, L.pw, L.ph);
        c.lineWidth = 1; c.font = `500 ${(0.62 * R2).toFixed(1)}px "JetBrains Mono", monospace`; c.textBaseline = "middle";
        for (let db = -84; db <= 0; db += 12) {
          if (db > DB_TOP || db < DB_BOT) continue;
          const y = Math.round(dy(db)) + 0.5;
          c.strokeStyle = db === 0 ? "rgba(255,255,255,.10)" : "rgba(255,255,255,.045)";
          c.beginPath(); c.moveTo(L.l, y); c.lineTo(L.l + L.pw, y); c.stroke();
          c.fillStyle = "#5d5a69"; c.textAlign = "right"; c.fillText(sign(String(db)), L.l - 0.5 * R2, y);
        }
        if (cfg.eqAxis !== false) {
          const stepDb = EQ >= 24 ? 12 : EQ >= 12 ? 6 : 3;
          for (let db = -EQ + (EQ % stepDb); db <= EQ; db += stepDb) {
            if (Math.abs(db) > EQ - 0.5 && db !== 0) continue;
            c.fillStyle = rgb("--accent-hot-rgb", 0.55); c.textAlign = "left"; c.fillText(sign((db > 0 ? "+" : "") + db), L.l + L.pw + 0.5 * R2, ey(db));
          }
          c.strokeStyle = rgb("--accent-rgb", 0.22); c.setLineDash([2, 4]);
          c.beginPath(); c.moveTo(L.l, Math.round(ey(0)) + 0.5); c.lineTo(L.l + L.pw, Math.round(ey(0)) + 0.5); c.stroke(); c.setLineDash([]);
        }
        c.textAlign = "center"; c.textBaseline = "top";
        for (const f of [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
          const x = Math.round(fx(f)) + 0.5;
          c.strokeStyle = [100, 1000, 10000].includes(f) ? "rgba(255,255,255,.09)" : "rgba(255,255,255,.04)";
          c.beginPath(); c.moveTo(x, L.t); c.lineTo(x, L.t + L.ph); c.stroke();
          c.fillStyle = "#5d5a69"; c.fillText(f >= 1000 ? `${f / 1000}k` : String(f), clamp(x, L.l + 1.0 * R2, L.l + L.pw - 0.8 * R2), L.t + L.ph + 0.45 * R2);
        }
        const stale = ctx.stale();
        const aUp = 1 - Math.exp(-dt / 0.012), aDn = 1 - Math.exp(-dt / (stale ? 0.12 : 0.3));
        for (let i = 0; i < BANDS; i++) {
          const ti = stale ? -120 : bandsIn[i], to = stale ? -120 : bandsOut[i];
          showIn[i] += (ti - showIn[i]) * (ti > showIn[i] ? aUp : aDn);
          showOut[i] += (to - showOut[i]) * (to > showOut[i] ? aUp : aDn);
          if (showOut[i] >= hold[i]) { hold[i] = showOut[i]; holdAge[i] = 0; } else { holdAge[i] += dt; if (holdAge[i] > 0.9) hold[i] -= 14 * dt; }
        }
        c.save(); c.beginPath(); c.rect(L.l, L.t, L.pw, L.ph); c.clip();
        if (cfg.under) cfg.under(c, S, dt);
        const curve = (arr, close) => {
          c.beginPath(); let px = 0, py = 0;
          for (let i = 0; i < BANDS; i++) {
            const f = bandHz(i), x = fx(f), y = dy(arr[i] + tiltOf(f));
            if (i === 0) { c.moveTo(L.l, y); c.lineTo(x, y); } else { const mx = (px + x) / 2, my = (py + y) / 2; c.quadraticCurveTo(px, py, mx, my); }
            px = x; py = y;
          }
          c.lineTo(px, py); c.lineTo(L.l + L.pw, py);
          if (close) { c.lineTo(L.l + L.pw, L.t + L.ph); c.lineTo(L.l, L.t + L.ph); c.closePath(); }
        };
        if (cfg.showInput !== false) {
          curve(showIn, true);
          const gi = c.createLinearGradient(0, L.t, 0, L.t + L.ph);
          gi.addColorStop(0, "rgba(150,156,180,.20)"); gi.addColorStop(1, "rgba(150,156,180,.02)");
          c.fillStyle = gi; c.fill();
          curve(showIn, false); c.strokeStyle = "rgba(170,176,200,.55)"; c.lineWidth = 1; c.stroke();
        }
        curve(showOut, true);
        const go = c.createLinearGradient(0, L.t, 0, L.t + L.ph);
        go.addColorStop(0, rgb("--accent-hot-rgb", 0.62)); go.addColorStop(0.45, rgb("--accent-rgb", 0.34)); go.addColorStop(1, rgb("--accent-dark-rgb", 0.04));
        c.fillStyle = go; c.fill();
        c.shadowColor = rgb("--accent-rgb", 0.9); c.shadowBlur = 0.7 * R2;
        curve(showOut, false); c.strokeStyle = css("--accent-soft") || "#d6beff"; c.lineWidth = 1.6; c.stroke(); c.shadowBlur = 0;
        c.beginPath();
        for (let i = 0; i < BANDS; i++) { const f = bandHz(i), x = fx(f), y = dy(hold[i] + tiltOf(f)); i ? c.lineTo(x, y) : c.moveTo(x, y); }
        c.strokeStyle = "rgba(255,255,255,.18)"; c.lineWidth = 1; c.stroke();
        const sr = meters.sr || 48000;
        // per-node curves, then the total
        if (cfg.bandCurve) {
          for (const n of nodes()) {
            if (n.hidden || !n.on) continue;
            c.beginPath();
            for (let i = 0; i <= 160; i++) { const f = FLO * Math.pow(FHI / FLO, i / 160); const y = ey(clamp(cfg.bandCurve(n, f, sr), -EQ, EQ)); i ? c.lineTo(fx(f), y) : c.moveTo(fx(f), y); }
            c.lineTo(L.l + L.pw, ey(0)); c.lineTo(L.l, ey(0)); c.closePath();
            const sel = S.selected === n.id;
            c.fillStyle = n.color ? n.color.replace("rgb(", "rgba(").replace(")", `,${sel ? 0.2 : 0.09})`) : `rgba(255,255,255,${sel ? 0.1 : 0.04})`;
            c.fill();
          }
        }
        if (cfg.curve) {
          c.beginPath();
          for (let i = 0; i <= 240; i++) { const f = FLO * Math.pow(FHI / FLO, i / 240); const y = ey(clamp(cfg.curve(f, sr), -EQ, EQ)); i ? c.lineTo(fx(f), y) : c.moveTo(fx(f), y); }
          c.shadowColor = "rgba(255,255,255,.55)"; c.shadowBlur = 0.5 * R2;
          c.strokeStyle = "rgba(255,255,255,.92)"; c.lineWidth = 1.5; c.stroke(); c.shadowBlur = 0;
        }
        if (cfg.over) cfg.over(c, S, dt);
        c.restore();
        // nodes
        for (const n of nodes()) {
          if (n.hidden) continue;
          const p = nodeXY(n), hot = (drag && drag.n.id === n.id) || (!drag && hover && hover.id === n.id) || S.selected === n.id;
          const r0 = (hot ? 0.56 : 0.46) * R2, x = clamp(p.x, L.l + r0, L.l + L.pw - r0), y = clamp(p.y, L.t + r0, L.t + L.ph - r0);
          const col = n.color || css("--accent-hot");
          c.globalAlpha = n.on === false ? 0.4 : 1;
          const g = c.createRadialGradient(x, y, 0, x, y, r0 * 2.6);
          g.addColorStop(0, col.replace("rgb(", "rgba(").replace(")", `,${hot ? 0.55 : 0.28})`)); g.addColorStop(1, "rgba(0,0,0,0)");
          c.fillStyle = g; c.beginPath(); c.arc(x, y, r0 * 2.6, 0, 7); c.fill();
          c.beginPath(); c.arc(x, y, r0, 0, 7); c.fillStyle = hot ? "#fff" : "#0a0a0f"; c.fill();
          c.lineWidth = 2; c.strokeStyle = col; c.stroke();
          if (n.badge) { c.font = `700 ${(0.52 * R2).toFixed(1)}px "JetBrains Mono", monospace`; c.textAlign = "center"; c.textBaseline = "middle"; c.fillStyle = hot ? "#000" : col; c.fillText(n.badge, x, y + 0.5); }
          if (n.ring != null && Math.abs(n.ring) > 0.05) {
            // dynamic gain: a second marker where the band sits right now
            const yy = ey(clamp((n.db ?? 0) + n.ring, -EQ, EQ));
            c.strokeStyle = col; c.lineWidth = 1.5; c.setLineDash([2, 2]); c.beginPath(); c.moveTo(x, y); c.lineTo(x, yy); c.stroke(); c.setLineDash([]);
            c.beginPath(); c.arc(x, yy, r0 * 0.5, 0, 7); c.fillStyle = col; c.fill();
          }
          c.globalAlpha = 1;
          if (hot && n.label) {
            const label = typeof n.label === "function" ? n.label() : n.label;
            c.font = `600 ${(0.64 * R2).toFixed(1)}px "JetBrains Mono", monospace`; c.textAlign = x > L.l + L.pw * 0.75 ? "right" : "left"; c.textBaseline = "middle";
            const tx = x + (c.textAlign === "left" ? 1 : -1) * (r0 + 0.5 * R2), tw = c.measureText(label).width;
            c.fillStyle = "rgba(6,6,10,.85)"; c.fillRect(c.textAlign === "left" ? tx - 0.35 * R2 : tx - tw - 0.35 * R2, y - 0.62 * R2, tw + 0.7 * R2, 1.24 * R2);
            c.fillStyle = "#fff"; c.fillText(label, tx, y);
          }
        }
        if (mouse.in && mouse.x >= L.l && mouse.x <= L.l + L.pw && !drag && !hover) {
          const f = fOfX(mouse.x), i = clamp(Math.round((Math.log(f / FLO) / Math.log(FHI / FLO)) * BANDS - 0.5), 0, BANDS - 1);
          c.strokeStyle = "rgba(255,255,255,.22)"; c.lineWidth = 1; c.beginPath(); c.moveTo(Math.round(mouse.x) + 0.5, L.t); c.lineTo(Math.round(mouse.x) + 0.5, L.t + L.ph); c.stroke();
          const names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
          const midi = 69 + 12 * Math.log2(f / 440), mr = Math.round(midi), cents = Math.round((midi - mr) * 100);
          const note = `${names[((mr % 12) + 12) % 12]}${Math.floor(mr / 12) - 1}${cents >= 0 ? "+" : MINUS}${Math.abs(cents)}c`;
          const text = `${f >= 1000 ? (f / 1000).toFixed(2) + " kHz" : f.toFixed(0) + " Hz"}  ${note}  out ${sign(Math.max(-99.9, showOut[i]).toFixed(1))} dB`;
          c.font = `500 ${(0.64 * R2).toFixed(1)}px "JetBrains Mono", monospace`; const tw = c.measureText(text).width;
          const bx = clamp(mouse.x + 0.8 * R2, L.l, L.l + L.pw - tw - 0.8 * R2), by = L.t + L.ph - 1.7 * R2;
          c.fillStyle = "rgba(6,6,10,.88)"; c.fillRect(bx, by, tw + 0.8 * R2, 1.3 * R2);
          c.fillStyle = "#d9d4e8"; c.textAlign = "left"; c.textBaseline = "middle"; c.fillText(text, bx + 0.4 * R2, by + 0.65 * R2);
        }
      }
      // node dragging: x -> fKey (log), y -> gKey (dB), wheel -> qKey
      canvas.addEventListener("pointermove", (e) => {
        const r = canvas.getBoundingClientRect(); mouse.x = e.clientX - r.left; mouse.y = e.clientY - r.top; mouse.in = true;
        if (drag) {
          const n = drag.n, fine = e.shiftKey ? 0.1 : 1;
          if (n.fKey) { drag.lf += ((mouse.x - drag.px) / L.pw) * LOGSPAN * fine; apf.set(n.fKey, normOf(n.fKey, FLO * Math.pow(10, drag.lf))); }
          if (n.gKey) { drag.db -= (mouse.y - drag.py) * ((2 * EQ) / L.ph) * fine * (n.gScale || 1); const rg = R[n.gKey]; drag.db = clamp(drag.db, rg ? rg.min : -EQ, rg ? rg.max : EQ); apf.set(n.gKey, normOf(n.gKey, drag.db)); }
          drag.px = mouse.x; drag.py = mouse.y;
        } else { hover = hit(mouse.x, mouse.y); canvas.style.cursor = hover ? (hover.gKey && hover.fKey ? "move" : hover.gKey ? "ns-resize" : "ew-resize") : "crosshair"; }
      });
      canvas.addEventListener("pointerleave", () => { mouse.in = false; hover = null; });
      canvas.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        const r = canvas.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top, n = hit(x, y);
        if (!n) { if (cfg.onEmptyClick) cfg.onEmptyClick(fOfX(x), dbOfEy(y), e); return; }
        S.selected = n.id; if (cfg.onSelect) cfg.onSelect(n);
        canvas.setPointerCapture(e.pointerId);
        const from = {};
        for (const k of [n.fKey, n.gKey]) if (k) { from[k] = P(k).value; apf.begin(k); }
        drag = { n, from, px: x, py: y, lf: n.fKey ? Math.log10(plainNow(n.fKey) / FLO) : 0, db: n.gKey ? plainNow(n.gKey) : 0 };
        e.preventDefault();
      });
      const endDrag = () => {
        if (!drag) return;
        const to = {};
        for (const k of Object.keys(drag.from)) { apf.end(k); to[k] = P(k).value; }
        recordMany(drag.from, to); drag = null;
      };
      canvas.addEventListener("pointerup", endDrag); canvas.addEventListener("pointercancel", endDrag);
      canvas.addEventListener("dblclick", (e) => {
        const r = canvas.getBoundingClientRect(), n = hit(e.clientX - r.left, e.clientY - r.top);
        if (!n) return;
        if (cfg.onNodeDblClick) { cfg.onNodeDblClick(n); return; }
        const from = {}, to = {};
        for (const k of [n.gKey]) if (k) { from[k] = P(k).value; to[k] = P(k).default; apf.change(k, to[k]); }
        recordMany(from, to);
      });
      let wheel = null;
      canvas.addEventListener("wheel", (e) => {
        const r = canvas.getBoundingClientRect(), n = hit(e.clientX - r.left, e.clientY - r.top) || (wheel && wheel.n);
        if (!n || !n.qKey) return;
        e.preventDefault();
        if (!wheel || wheel.n.id !== n.id) { wheel = { n, from: P(n.qKey).value, v: P(n.qKey).value, t: 0 }; apf.begin(n.qKey); }
        wheel.v = clamp01(wheel.v + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 0.004 : 0.025));
        apf.set(n.qKey, wheel.v);
        clearTimeout(wheel.t);
        wheel.t = setTimeout(() => { apf.end(n.qKey); recordParam(n.qKey, wheel.from, P(n.qKey).value); wheel = null; }, 280);
      }, { passive: false });
      if (cfg.hint) { canvas.addEventListener("pointerenter", () => hint(cfg.hint)); canvas.addEventListener("pointerleave", () => hint("")); }
      S.draw = draw;
      return S;
    };

    /* level meters: IN, OUT, GR (optional) */
    ctx.levelMeters = (canvas, cfg = {}) => {
      const mctx = canvas.getContext("2d");
      const M = { inPk: [-120, -120], outPk: [-120, -120], inRms: [-120, -120], outRms: [-120, -120], inHold: [-120, -120], outHold: [-120, -120], inAge: [0, 0], outAge: [0, 0], inClip: false, outClip: false, grShown: 0, outMax: -120, inMax: -120 };
      canvas.addEventListener("click", () => { M.inClip = M.outClip = false; M.inMax = M.outMax = -120; M.inHold = [-120, -120]; M.outHold = [-120, -120]; });
      const TOP = 3, BOT = -60, GRMAX = cfg.grRange || 12;
      const showGr = cfg.gr !== false;
      function draw(dt) {
        const c = mctx, R2 = rem();
        const s = K.sizeCanvas(canvas, c), w = s.w, hh = s.h;
        c.clearRect(0, 0, w, hh);
        const stale = ctx.stale();
        const upd = (shown, tgt, dnDb) => (tgt > shown ? tgt : Math.max(tgt, shown - dnDb * dt));
        for (let ch = 0; ch < 2; ch++) {
          const ip = stale ? -120 : lin2db(meters.inPk[ch]), op = stale ? -120 : lin2db(meters.outPk[ch]);
          M.inPk[ch] = upd(M.inPk[ch], ip, 55); M.outPk[ch] = upd(M.outPk[ch], op, 55);
          const ir = stale ? -120 : lin2db(meters.inRms[ch]), orr = stale ? -120 : lin2db(meters.outRms[ch]);
          M.inRms[ch] += (ir - M.inRms[ch]) * (1 - Math.exp(-dt / (ir > M.inRms[ch] ? 0.04 : 0.28)));
          M.outRms[ch] += (orr - M.outRms[ch]) * (1 - Math.exp(-dt / (orr > M.outRms[ch] ? 0.04 : 0.28)));
          for (const [hl, pk, age] of [[M.inHold, M.inPk, M.inAge], [M.outHold, M.outPk, M.outAge]]) { if (pk[ch] >= hl[ch]) { hl[ch] = pk[ch]; age[ch] = 0; } else { age[ch] += dt; if (age[ch] > 1.6) hl[ch] -= 18 * dt; } }
          if (ip > -0.1) M.inClip = true; if (op > -0.1) M.outClip = true;
          M.inMax = Math.max(M.inMax, ip); M.outMax = Math.max(M.outMax, op);
        }
        const gdb = stale ? 0 : Math.min(0, lin2db(meters.gr));
        M.grShown = gdb < M.grShown ? gdb : M.grShown + (gdb - M.grShown) * (1 - Math.exp(-dt / 0.35));
        const barW = 0.72 * R2, gap = 0.14 * R2, top = 2.2 * R2, bot = hh - 2.9 * R2, ph = Math.max(10, bot - top);
        const groupW = barW * 2 + gap, scaleW = 1.4 * R2, grW = showGr ? barW + 1.5 * R2 : 0;
        const total = groupW * 2 + scaleW + grW + 0.35 * R2;
        const pad = Math.max(0.4 * R2, (w - total) / 2);
        const y = (db) => top + ph * (1 - clamp01((db - BOT) / (TOP - BOT)));
        const inX = [pad, pad + barW + gap], scaleX = inX[1] + barW + 0.15 * R2, outX = [scaleX + scaleW, scaleX + scaleW + barW + gap], grX = outX[1] + barW + 0.55 * R2;
        const font = (px, wt = 500) => `${wt} ${(px * R2).toFixed(1)}px "JetBrains Mono", monospace`;
        c.textBaseline = "middle"; c.font = font(0.62, 600); c.fillStyle = "#8b8797"; c.textAlign = "center";
        c.fillText("IN", inX[0] + groupW / 2, 1.0 * R2); c.fillText("OUT", outX[0] + groupW / 2, 1.0 * R2);
        if (showGr) c.fillText(cfg.grLabel || "GR", grX + barW / 2, 1.0 * R2);
        const grad = c.createLinearGradient(0, y(TOP), 0, y(BOT));
        grad.addColorStop(0, "#ff4f6d"); grad.addColorStop(0.07, "#ffbf4a"); grad.addColorStop(0.2, css("--accent-hot")); grad.addColorStop(0.55, css("--accent")); grad.addColorStop(1, css("--accent-dark"));
        const bar = (x, rms, pk, hold) => {
          c.fillStyle = "#08080c"; c.fillRect(x, top, barW, ph);
          c.strokeStyle = "rgba(255,255,255,.06)"; c.lineWidth = 1; c.strokeRect(x + 0.5, top + 0.5, barW - 1, ph - 1);
          c.save(); c.beginPath(); c.rect(x + 1, top + 1, barW - 2, ph - 2); c.clip();
          c.globalAlpha = 0.5; c.fillStyle = grad; c.fillRect(x + 1, y(rms), barW - 2, bot - y(rms));
          c.globalAlpha = 1; c.fillRect(x + barW * 0.28, y(pk), barW * 0.44, bot - y(pk));
          c.restore();
          c.fillStyle = hold > -0.5 ? "#ff4f6d" : "#fff"; c.fillRect(x + 1, Math.round(y(hold)) - 1, barW - 2, 2);
        };
        for (let ch = 0; ch < 2; ch++) { bar(inX[ch], M.inRms[ch], M.inPk[ch], M.inHold[ch]); bar(outX[ch], M.outRms[ch], M.outPk[ch], M.outHold[ch]); }
        c.font = font(0.58); c.textAlign = "center"; c.fillStyle = "#5d5a69";
        for (const db of [0, -6, -12, -18, -24, -36, -48, -60]) {
          const yy = y(db); c.fillText(sign(String(db)), scaleX + scaleW / 2 - 0.1 * R2, yy);
          c.strokeStyle = "rgba(255,255,255,.05)"; c.beginPath(); c.moveTo(inX[0], Math.round(yy) + 0.5); c.lineTo(inX[1] + barW, Math.round(yy) + 0.5); c.stroke();
          c.beginPath(); c.moveTo(outX[0], Math.round(yy) + 0.5); c.lineTo(outX[1] + barW, Math.round(yy) + 0.5); c.stroke();
        }
        for (const [x0, clip] of [[inX[0], M.inClip], [outX[0], M.outClip]]) {
          c.fillStyle = clip ? "#ff4f6d" : "#1a1a22"; if (clip) { c.shadowColor = "#ff4f6d"; c.shadowBlur = 8; }
          c.fillRect(x0, 1.65 * R2, groupW, 0.3 * R2); c.shadowBlur = 0;
        }
        if (showGr) {
          c.fillStyle = "#08080c"; c.fillRect(grX, top, barW, ph); c.strokeStyle = "rgba(255,255,255,.06)"; c.strokeRect(grX + 0.5, top + 0.5, barW - 1, ph - 1);
          const gh = ph * clamp01(-M.grShown / GRMAX);
          const gg = c.createLinearGradient(0, top, 0, top + ph); gg.addColorStop(0, "#ffbf4a"); gg.addColorStop(1, "#ff4f6d");
          c.fillStyle = gg; c.fillRect(grX + 1, top + 1, barW - 2, Math.max(0, gh - 1));
          c.font = font(0.52); c.fillStyle = "#5d5a69"; c.textAlign = "left";
          for (const db of [0, -GRMAX / 4, -GRMAX / 2, -GRMAX]) c.fillText(sign(String(db)), grX + barW + 0.18 * R2, top + ph * (-db / GRMAX) + (db === 0 ? 0.3 * R2 : 0));
        }
        const rd = (db) => (db <= -99 ? "−∞" : sign(db.toFixed(1)));
        c.font = font(0.66, 600); c.textAlign = "center";
        c.fillStyle = M.inMax > -0.1 ? "#ff4f6d" : "#d9d4e8"; c.fillText(rd(M.inMax), inX[0] + groupW / 2, bot + 1.15 * R2);
        c.fillStyle = M.outMax > -0.1 ? "#ff4f6d" : "#d9d4e8"; c.fillText(rd(M.outMax), outX[0] + groupW / 2, bot + 1.15 * R2);
        if (showGr) { c.fillStyle = M.grShown < -0.05 ? "#ffbf4a" : "#6a6677"; c.fillText(M.grShown < -0.05 ? sign(M.grShown.toFixed(1)) : "0.0", grX + barW / 2, bot + 1.15 * R2); }
        c.font = font(0.5); c.fillStyle = "#44414f";
        c.fillText("PEAK dBFS · click to clear", w / 2, bot + 2.2 * R2);
        if (cfg.after) cfg.after(c, { w, h: hh, top, bot, R2 });
      }
      return { draw, M };
    };

    /* the plugin's own parts */
    const drawFns = [];
    ctx.onDraw = (fn) => drawFns.push(fn);
    if (opts.init) await opts.init(ctx);
    if (ctx.ext && ctx.ext.refresh) { try { await ctx.ext.refresh(); } catch (e) { apf.log("ext:", String(e)); } }
    $$(".knob").forEach(attachKnob);
    bindChoices(document);
    bindHistory(document);
    committed = snapshot();

    /* A/B compare */
    const ab = { active: "A", A: null, B: null };
    const abA = $("#ab-a"), abB = $("#ab-b");
    const showAb = () => { abA.classList.toggle("on", ab.active === "A"); abB.classList.toggle("on", ab.active === "B"); abA.setAttribute("aria-pressed", ab.active === "A"); abB.setAttribute("aria-pressed", ab.active === "B"); };
    let abTimer = 0, abPending = false;
    function scheduleAb() { abPending = true; clearTimeout(abTimer); abTimer = setTimeout(saveAb, 400); }
    function saveAb() { abPending = false; ab[ab.active] = snapshot(); apf.plugin.post("ab", { active: ab.active, A: ab.A, B: ab.B }).catch((e) => apf.log("ab:", String(e))); }
    async function loadAb() {
      try { const v = await apf.plugin.get("ab"); if (v && typeof v === "object" && !abPending) { ab.active = v.active === "B" ? "B" : "A"; ab.A = v.A || null; ab.B = v.B || null; } } catch (e) { /* none yet */ }
      showAb();
    }
    function switchTo(slot) {
      if (slot === ab.active) return;
      ab[ab.active] = snapshot();
      const target = ab[slot] || ab[ab.active];
      ab.active = slot; showAb();
      replay(() => applySnap(target));
      ab[slot] = target; scheduleAb();
    }
    abA.addEventListener("click", () => switchTo("A"));
    abB.addEventListener("click", () => switchTo("B"));
    $("#ab-copy").addEventListener("click", () => {
      const other = ab.active === "A" ? "B" : "A";
      ab[other] = snapshot(); saveAb();
      (other === "A" ? abA : abB).animate([{ background: rgb("--accent-rgb", 0.7) }, { background: "transparent" }], { duration: 500 });
    });
    await loadAb();

    /* presets in the history; session changes from the host */
    let lastPreset;
    apf.presets.on((p) => {
      const id = p ? p.id : null;
      if (lastPreset === undefined) { lastPreset = id; return; }
      if (id === lastPreset) return;
      lastPreset = id;
      if (replaying) return;
      const before = committed;
      setTimeout(async () => {
        if (ctx.ext && ctx.ext.refresh) { try { await ctx.ext.refresh(); } catch (e) { /* closing */ } }
        const after = snapshot(); committed = after;
        H.u.push({ undo: () => applySnap(before), redo: () => applySnap(after) }); H.r.length = 0; syncHist();
        scheduleAb();
      }, 200);
    });
    apf.plugin.onChange(() => { if (ctx.ext && ctx.ext.refresh) ctx.ext.refresh().catch(() => {}); loadAb(); });

    /* footer */
    const fLat = $("#f-lat"), fSr = $("#f-sr"), fTr = $("#f-tr");
    let fText = "";
    function footer(sr, lat, playing) {
      const t = `${sr}|${lat}|${playing}`;
      if (t === fText) return; fText = t;
      fSr.textContent = sr ? `${(sr / 1000).toFixed(sr % 1000 ? 1 : 0)} kHz` : "— kHz";
      fLat.textContent = sr ? `Latency ${((lat / sr) * 1000).toFixed(1)} ms · ${Math.round(lat)} smp` : "Latency —";
      fTr.innerHTML = `<i class="dot${playing ? " on" : ""}"></i><b>${playing ? "Playing" : "Stopped"}</b>${!playing && opts.stoppedNote ? ` · ${opts.stoppedNote}` : ""}`;
    }

    apf.poller(async () => ingest(await apf.plugin.raw("frame")), { fast: 33, slow: 100, idle: 250 });
    let last = performance.now();
    function loop(now) {
      const dt = Math.min(0.1, Math.max(0.001, (now - last) / 1000)); last = now;
      for (const fn of drawFns) { try { fn(dt); } catch (e) { apf.log("draw:", String(e && e.stack || e)); } }
      footer(meters.sr, meters.lat, meters.playing > 0);
      requestAnimationFrame(loop);
    }
    requestAnimationFrame(loop);

    /* trial and license (state from nodaw-kit's license.rs) */
    const name = SHELL.name;
    const lic = { s: null, timer: 0, shownEnded: false };
    const licBadge = $("#lic-badge"), licDlg = $("#lic"), licKey = $("#lic-key"), licBtn = $("#lic-activate"), licMsg = $("#lic-msg");
    function licRender() {
      const s = lic.s; if (!s) return;
      const st = s.state, ended = st === "expired" || st === "tampered", licensed = st === "licensed";
      licBadge.hidden = licensed;
      licBadge.classList.toggle("ended", ended);
      licBadge.classList.toggle("low", st === "trial" && s.days_left <= 3);
      $(".lic-txt", licBadge).textContent = ended ? "Trial ended · Activate" : `Trial · ${s.days_left} ${s.days_left === 1 ? "day" : "days"} left`;
      licDlg.classList.toggle("ended", ended);
      licDlg.classList.toggle("licensed", licensed);
      $("#lic-title").textContent = licensed ? "Activated" : ended ? "Trial ended" : `${s.days_left} ${s.days_left === 1 ? "day" : "days"} left`;
      $("#lic-bar").style.width = licensed || ended ? "100%" : `${Math.max(4, (100 * s.days_left) / (s.trial_days || 14))}%`;
      $("#lic-sub").textContent = licensed
        ? "Every feature, for good. Thank you for supporting independent tools."
        : ended
          ? (st === "tampered" ? "The license file was changed, so the trial has ended. " : "") + `${name} keeps working, but the output drops out for half a second every 45 seconds. Activate to remove it. Your sessions and presets are safe.`
          : "Every feature is unlocked during the trial. After it, the output drops out briefly every 45 seconds until you activate.";
      $("#lic-licensed").hidden = !licensed;
      $("#lic-who").textContent = s.email ? `Licensed to ${s.email}` : "Licensed";
      $("#lic-keyhint").textContent = s.key_hint ? `Key ${s.key_hint}` : "";
      $("#lic-fine").textContent = s.activation_ready === false ? `Activation opens when ${name} goes on sale.` : "Your key is in the Gumroad receipt email.";
      licBtn.disabled = !!s.busy; licBtn.classList.toggle("busy", !!s.busy); licBtn.textContent = s.busy ? "Checking…" : "Activate";
      if (s.busy) { licMsg.className = "lic-msg"; licMsg.textContent = "Checking your key with Gumroad…"; }
      else if (s.message) { licMsg.className = `lic-msg ${s.ok ? "ok" : "err"}`; licMsg.textContent = s.message; }
      if (ended && !lic.shownEnded) { lic.shownEnded = true; licOpen(); }
    }
    async function licPoll() {
      clearTimeout(lic.timer);
      try { lic.s = await apf.plugin.get("license"); licRender(); } catch (e) { apf.log("license:", String(e)); }
      lic.timer = setTimeout(licPoll, lic.s && lic.s.busy ? 600 : 60_000);
    }
    function licOpen() { licDlg.hidden = false; if (!licDlg.classList.contains("licensed")) setTimeout(() => licKey.focus(), 30); }
    function licClose() { licDlg.hidden = true; }
    licBadge.addEventListener("click", licOpen);
    $("#lic-close").addEventListener("click", licClose);
    licDlg.addEventListener("pointerdown", (e) => { if (e.target === licDlg) licClose(); });
    licDlg.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.preventDefault(); licClose(); } e.stopPropagation(); });
    $("#lic-form").addEventListener("submit", async (e) => {
      e.preventDefault();
      const key = licKey.value.trim();
      if (!key) { licMsg.className = "lic-msg err"; licMsg.textContent = "Paste the license key from your Gumroad receipt."; return; }
      try { lic.s = await apf.plugin.post("license-activate", { key }); licRender(); licPoll(); }
      catch (err) { licMsg.className = "lic-msg err"; licMsg.textContent = (err && err.body) || "Could not start activation."; }
    });
    $("#lic-buy").addEventListener("click", async () => {
      try { const r = await apf.plugin.post("license-buy", {}); if (!r.opened) { licMsg.className = "lic-msg"; licMsg.textContent = `Open ${r.url} in your browser.`; } }
      catch (e) { apf.log("buy:", String(e)); }
    });
    licPoll();

    window.__nodaw = Object.assign(ctx, { ab, lic });
    return ctx;
  });
})();
