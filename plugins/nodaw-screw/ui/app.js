/* NoDAW Screw: plugin UI.
 *
 * Every control is bound to a host parameter through apf.js (data-param, apf.begin/set/end),
 * so the host sees the gestures: automation recording, undo in the host, MIDI learn. The only
 * state that is not a parameter is the Custom chop pattern and the A/B slots; both live in
 * the plugin's controller and are saved with the session and with presets.
 *
 * Data from the audio thread (meters, spectra) arrives as one binary frame about every
 * 33 ms from the controller; drawing runs at the display's rate with its own ballistics.
 */
(() => {
  "use strict";

  /* ── small helpers ───────────────────────────────────────────────────────────────── */
  const $ = (s, r = document) => r.querySelector(s);
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const clamp01 = (v) => clamp(v, 0, 1);
  const lerp = (a, b, t) => a + (b - a) * t;
  const NS = "http://www.w3.org/2000/svg";
  const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize) || 13;
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const MINUS = "\u2212";
  const sign = (s) => s.replace("-", MINUS);

  // The whole page scales with the window: 960x600 is the design size (13 px root).
  function fitRoot() {
    const u = Math.min(innerWidth / 960, innerHeight / 600);
    document.documentElement.style.fontSize = `${13 * clamp(u, 0.74, 2.8)}px`;
  }
  fitRoot();
  addEventListener("resize", fitRoot);

  /* ── the knob table: key, hint. Order is the layout. ─────────────────────────────── */
  const KNOBS = {
    "g-screw": [
      { k: "pitch", hero: true, bipolar: true, hint: "Pitch and speed together, like slowing a record. Alt-drag snaps to whole semitones." },
      { k: "wow", hint: "Slow, tape-style pitch drift (depth in cents)." },
      { k: "wow_rate", hint: "How fast the wow cycles." },
      { k: "flutter", hint: "Fast random pitch shimmer." },
      { k: "lowpass", hint: "Darkens the top end. You can also drag the node on the analyzer." },
    ],
    "g-space": [
      { k: "bass", bipolar: true, hint: "Warmth: boost or cut everything below 150 Hz. Drag the node on the analyzer too." },
      { k: "rev_mix", hint: "How much reverb is added." },
      { k: "rev_decay", hint: "How long the reverb rings out." },
      { k: "rev_size", hint: "Room size." },
      { k: "rev_damp", hint: "Where the reverb's highs are damped. Lower is darker." },
      { k: "rev_pre", hint: "Pre-delay before the reverb starts." },
    ],
    "g-out": [
      { k: "in_gain", bipolar: true, hint: "Input gain before the effect." },
      { k: "mix", hint: "Dry/wet. The dry path is delay-compensated, so mixing never smears." },
      { k: "out_gain", bipolar: true, hint: "Output gain after the effect, before the limiter." },
      { k: "width", hint: "Stereo width of the wet signal. 100% leaves it unchanged." },
      { k: "ceiling", hint: "True-peak ceiling of the limiter." },
    ],
  };
  const DEFAULT_HINT = $("#f-hint").textContent;
  const hint = (t) => ($("#f-hint").textContent = t || DEFAULT_HINT);

  /* ── SVG knob ────────────────────────────────────────────────────────────────────── */
  const A0 = 135, SWEEP = 270;
  const pt = (r, deg) => [50 + r * Math.cos((deg * Math.PI) / 180), 50 + r * Math.sin((deg * Math.PI) / 180)];
  function arc(r, a0, a1) {
    if (Math.abs(a1 - a0) < 0.05) return "";
    const [x0, y0] = pt(r, a0), [x1, y1] = pt(r, a1);
    const large = Math.abs(a1 - a0) > 180 ? 1 : 0;
    const sweep = a1 > a0 ? 1 : 0;
    return `M${x0.toFixed(2)} ${y0.toFixed(2)}A${r} ${r} 0 ${large} ${sweep} ${x1.toFixed(2)} ${y1.toFixed(2)}`;
  }
  function el(tag, attrs, parent) {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  }
  function svgDefs() {
    const s = el("svg", { width: 0, height: 0, "aria-hidden": "true", style: "position:absolute" }, document.body);
    const d = el("defs", {}, s);
    const lg = el("linearGradient", { id: "kgrad", x1: 0, y1: 1, x2: 1, y2: 0 }, d);
    el("stop", { offset: 0, "stop-color": css("--accent-2") }, lg);
    el("stop", { offset: 0.55, "stop-color": css("--accent") }, lg);
    el("stop", { offset: 1, "stop-color": css("--accent-hot") }, lg);
    const body = el("radialGradient", { id: "kbody", cx: 0.5, cy: 0.32, r: 0.8 }, d);
    el("stop", { offset: 0, "stop-color": "#4a4a58" }, body);
    el("stop", { offset: 0.55, "stop-color": "#1f1f27" }, body);
    el("stop", { offset: 1, "stop-color": "#0d0d12" }, body);
    const rim = el("linearGradient", { id: "krim", x1: 0, y1: 0, x2: 0, y2: 1 }, d);
    el("stop", { offset: 0, "stop-color": "rgba(255,255,255,.55)" }, rim);
    el("stop", { offset: 0.45, "stop-color": "rgba(255,255,255,.05)" }, rim);
    el("stop", { offset: 1, "stop-color": "rgba(255,255,255,.18)" }, rim);
    const cap = el("radialGradient", { id: "kcap", cx: 0.4, cy: 0.3, r: 0.9 }, d);
    el("stop", { offset: 0, "stop-color": "#2e2e39" }, cap);
    el("stop", { offset: 1, "stop-color": "#121217" }, cap);
    const sh = el("radialGradient", { id: "kshadow" }, d);
    el("stop", { offset: 0.7, "stop-color": "rgba(0,0,0,.65)" }, sh);
    el("stop", { offset: 1, "stop-color": "rgba(0,0,0,0)" }, sh);
  }

  function knobCell(spec) {
    const cell = document.createElement("div");
    cell.className = `knob${spec.hero ? " hero" : ""}${spec.mini ? " mini" : ""}`;
    cell.dataset.key = spec.k;
    cell.setAttribute("data-param-learn", spec.k);
    const face = document.createElement("div");
    face.className = "k-face";
    face.tabIndex = 0;
    face.setAttribute("role", "slider");
    face.setAttribute("aria-valuemin", "0");
    face.setAttribute("aria-valuemax", "1");
    const svg = el("svg", { viewBox: "0 0 100 100" }, face);
    for (let i = 0; i <= 10; i++) {
      const a = A0 + (SWEEP * i) / 10;
      const [x0, y0] = pt(i % 5 === 0 ? 44.5 : 45.5, a), [x1, y1] = pt(48.5, a);
      el("line", { class: `k-tick${i % 5 === 0 ? " maj" : ""}`, x1: x0, y1: y0, x2: x1, y2: y1, "stroke-width": 1.4, "stroke-linecap": "round" }, svg);
    }
    el("path", { class: "k-track", d: arc(40, A0, A0 + SWEEP), fill: "none", "stroke-width": 5, "stroke-linecap": "round" }, svg);
    el("path", { class: "k-ring", d: "", fill: "none", "stroke-width": 5, "stroke-linecap": "round" }, svg);
    el("circle", { cx: 50, cy: 54, r: 36, fill: "url(#kshadow)" }, svg);
    el("circle", { cx: 50, cy: 50, r: 31, fill: "url(#kbody)", stroke: "#000", "stroke-width": 1 }, svg);
    el("circle", { cx: 50, cy: 50, r: 30, fill: "none", stroke: "url(#krim)", "stroke-width": 1.1 }, svg);
    el("circle", { cx: 50, cy: 50, r: 22.5, fill: "url(#kcap)", stroke: "rgba(0,0,0,.7)", "stroke-width": 0.8 }, svg);
    el("circle", { cx: 50, cy: 50, r: 21.7, fill: "none", stroke: "rgba(255,255,255,.07)", "stroke-width": 0.8 }, svg);
    el("line", { class: "k-ptr", x1: 50, y1: 50, x2: 50, y2: 50, "stroke-width": 3.6, "stroke-linecap": "round" }, svg);
    const label = document.createElement("div");
    label.className = "k-label";
    label.setAttribute("data-param-name", spec.k);
    const value = document.createElement("div");
    value.className = "k-value";
    value.setAttribute("data-param-text", spec.k);
    value.title = "Click to type a value";
    cell.append(face, label, value);
    return cell;
  }

  // The depth knob of the chop strip is built like the others.
  function buildKnobs() {
    svgDefs();
    for (const [id, list] of Object.entries(KNOBS)) {
      const host = $(`#${id}`);
      const before = id === "g-out" ? $("#limiter") : null;
      for (const spec of list) {
        const c = knobCell(spec);
        c.dataset.hint = spec.hint || "";
        if (spec.bipolar) c.dataset.bipolar = "1";
        host.insertBefore(c, before);
        if (spec.hero) c.classList.add("hero");
      }
    }
    const d = knobCell({ k: "chop_depth", mini: true });
    d.dataset.hint = "How far the closed steps duck the signal. 100% is a full gate.";
    $("#chop-depth-slot").appendChild(d);
  }
  buildKnobs();

  /* ── everything below needs the parameter table ──────────────────────────────────── */
  apf.ready.then(async () => {
    const app = $("#app");
    const params = apf.params;
    const keyIndex = Object.fromEntries(params.map((p, i) => [p.key, i]));
    const P = (k) => apf.param(k);

    // Plain <-> host value maps, from the plugin (the same table the DSP uses).
    let R = {};
    try { R = await apf.plugin.get("ranges"); } catch (e) { apf.log("ranges:", String(e)); }
    const plainOf = (k, n) => {
      const r = R[k]; if (!r) return n; n = clamp01(n);
      if (r.curve === "log") return r.min * Math.pow(r.max / r.min, n);
      if (r.curve === "pow") return r.min + (r.max - r.min) * Math.pow(n, r.exp);
      return r.min + (r.max - r.min) * n;
    };
    const normOf = (k, v) => {
      const r = R[k]; if (!r || r.max === r.min) return 0;
      v = clamp(v, Math.min(r.min, r.max), Math.max(r.min, r.max));
      if (r.curve === "log") return clamp01(Math.log(v / r.min) / Math.log(r.max / r.min));
      if (r.curve === "pow") return clamp01(Math.pow((v - r.min) / (r.max - r.min), 1 / r.exp));
      return clamp01((v - r.min) / (r.max - r.min));
    };
    const plainNow = (k) => plainOf(k, P(k).value);
    // Accept what the user types: "-3", "9k", "1.2 kHz", "40%", "12 dB".
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

    /* ── history: undo / redo of user edits ──────────────────────────────────────── */
    const H = { u: [], r: [] };
    let replaying = false;
    const undoBtn = $("#undo"), redoBtn = $("#redo");
    const syncHist = () => { undoBtn.disabled = !H.u.length; redoBtn.disabled = !H.r.length; };
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
    const recordParam = (key, from, to) => {
      if (Math.abs(from - to) < 1e-9) return;
      push({ undo: () => apf.change(key, from), redo: () => apf.change(key, to) });
    };

    /* ── snapshots (A/B, presets in the history) ────────────────────────────────── */
    let custom = new Array(16).fill(2);
    let patterns = [];
    const snapshot = () => {
      const p = {};
      for (const d of params) if (d.key !== "bypass") p[d.key] = P(d.key).value;
      return { p, s: custom.slice() };
    };
    let committed = snapshot();
    function applySnap(sn) {
      for (const [k, v] of Object.entries(sn.p)) {
        if (!(k in keyIndex)) continue;
        if (Math.abs(P(k).value - v) > 1e-9) apf.change(k, v);
      }
      if (sn.s && sn.s.join() !== custom.join()) { custom = sn.s.slice(); postSteps(); renderSteps(); }
    }

    /* ── knobs ───────────────────────────────────────────────────────────────────── */
    const tip = $("#tip");
    let tipTimer = 0;
    function showTip(cell, key) {
      const r = $(".k-face", cell).getBoundingClientRect();
      tip.innerHTML = `<span class="t-n"></span><span class="t-v"></span>`;
      tip.firstChild.textContent = P(key).name;
      tip.style.left = `${r.left + r.width / 2}px`;
      tip.style.top = `${r.top}px`;
      updateTip(key);
      tip.classList.add("show");
      clearTimeout(tipTimer);
    }
    const updateTip = (key) => { const v = $(".t-v", tip); if (v) v.textContent = sign(P(key).text || ""); };
    const hideTip = (delay = 0) => { clearTimeout(tipTimer); tipTimer = setTimeout(() => tip.classList.remove("show"), delay); };

    const knobs = {};
    function attachKnob(cell) {
      const key = cell.dataset.key;
      const p = P(key);
      if (!p) return;
      const face = $(".k-face", cell), ring = $(".k-ring", cell), ptr = $(".k-ptr", cell), valueBox = $(".k-value", cell);
      const bipolar = cell.dataset.bipolar && R[key] ? normOf(key, 0) : null;
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
        if (key === "pitch" && (e.altKey || e.ctrlKey)) return normOf("pitch", Math.round(plainOf("pitch", v)));
        return v;
      };
      face.addEventListener("pointerdown", (e) => {
        if (e.button !== 0 || e.ctrlKey && e.pointerType === "mouse" && navigator.platform.startsWith("Mac")) return;
        face.setPointerCapture(e.pointerId);
        drag = { x: e.clientX, y: e.clientY, v: p.value, from: p.value };
        apf.begin(key); cell.classList.add("active"); showTip(cell, key); face.focus({ preventScroll: true });
        e.preventDefault();
      });
      face.addEventListener("pointermove", (e) => {
        if (!drag) return;
        const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
        drag.x = e.clientX; drag.y = e.clientY;
        // About 260 px of travel for the whole range; shift gives 8x finer.
        const per = (e.shiftKey ? 1 / 2100 : 1 / 260) * (cell.classList.contains("hero") ? 0.8 : 1);
        drag.v = clamp01(drag.v + (dx - dy) * per);
        apf.set(key, detent(drag.v, e));
      });
      const finish = () => {
        if (!drag) return;
        apf.end(key); cell.classList.remove("active"); hideTip(450);
        recordParam(key, drag.from, p.value); drag = null;
      };
      face.addEventListener("pointerup", finish);
      face.addEventListener("pointercancel", finish);
      face.addEventListener("lostpointercapture", finish);
      face.addEventListener("dblclick", () => { const from = p.value; apf.change(key, p.default); recordParam(key, from, p.default); });
      face.addEventListener("contextmenu", (e) => { /* the framework's menu: MIDI learn, default value */ if (drag) e.preventDefault(); });

      // wheel: one history entry per burst
      let wheel = null;
      face.addEventListener("wheel", (e) => {
        e.preventDefault();
        if (!wheel) { wheel = { from: p.value, v: p.value, t: 0 }; apf.begin(key); }
        wheel.v = clamp01(wheel.v + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 0.002 : 0.012));
        apf.set(key, detent(wheel.v, e));
        cell.classList.add("active"); showTip(cell, key);
        clearTimeout(wheel.t);
        wheel.t = setTimeout(() => { apf.end(key); cell.classList.remove("active"); hideTip(250); recordParam(key, wheel.from, p.value); wheel = null; }, 280);
      }, { passive: false });

      face.addEventListener("keydown", (e) => {
        const big = e.key === "PageUp" || e.key === "PageDown";
        const dir = e.key === "ArrowUp" || e.key === "ArrowRight" || e.key === "PageUp" ? 1 : e.key === "ArrowDown" || e.key === "ArrowLeft" || e.key === "PageDown" ? -1 : 0;
        let v = null;
        if (dir) v = clamp01(p.value + dir * (big ? 0.1 : e.shiftKey ? 0.002 : 0.01));
        else if (e.key === "Home") v = 0;
        else if (e.key === "End") v = 1;
        else if (e.key === "Enter") { beginEdit(); e.preventDefault(); return; }
        if (v === null) return;
        e.preventDefault();
        const from = p.value; apf.change(key, v); recordParam(key, from, v);
      });

      // typed values
      function beginEdit() {
        if ($("input", valueBox)) return;
        const r = R[key];
        const input = document.createElement("input");
        input.type = "text"; input.value = sign(p.text || ""); input.setAttribute("aria-label", `${p.name} value`);
        const keep = valueBox.textContent;
        valueBox.removeAttribute("data-param-text");
        valueBox.textContent = ""; valueBox.appendChild(input);
        input.focus(); input.select();
        let done = false;
        const end = (commit) => {
          if (done) return; done = true;
          valueBox.textContent = keep; valueBox.setAttribute("data-param-text", key); valueBox.textContent = p.text || keep;
          if (commit) {
            const n = parseTyped(key, input.value);
            if (n !== null && r) { const from = p.value; apf.change(key, n); recordParam(key, from, n); }
          }
          face.focus({ preventScroll: true });
        };
        input.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Enter") end(true); else if (e.key === "Escape") end(false); });
        input.addEventListener("blur", () => end(true));
      }
      valueBox.addEventListener("click", beginEdit);
    }
    document.querySelectorAll(".knob").forEach(attachKnob);

    /* ── selects, switches: history + hints ──────────────────────────────────────── */
    const cur = {}, prev = {};
    for (const d of params) { cur[d.key] = prev[d.key] = d.value; apf.on(d.key, (v) => { if (v !== cur[d.key]) { prev[d.key] = cur[d.key]; cur[d.key] = v; } }); }
    document.querySelectorAll("select[data-param], input[type=checkbox][data-param]").forEach((input) => {
      const key = input.dataset.param;
      if (key === "bypass") return;
      input.addEventListener("change", () => recordParam(key, prev[key], cur[key]));
    });
    apf.on("bypass", (v) => { app.classList.toggle("bypassed", v >= 0.5); });

    /* ── chop step editor ────────────────────────────────────────────────────────── */
    const stepsBox = $("#steps");
    const cells = [];
    for (let i = 0; i < 16; i++) {
      const c = document.createElement("div");
      c.className = "step"; c.dataset.i = i; c.dataset.l = 2;
      c.appendChild(document.createElement("b"));
      stepsBox.appendChild(c); cells.push(c);
    }
    const patParam = P("chop_pattern");
    const CUSTOM = patParam.options.length - 1;
    const patIndex = () => Math.round(patParam.value * patParam.steps);
    function shownPattern() { const i = patIndex(); return i === CUSTOM ? custom : (patterns[i] && patterns[i].steps) || new Array(16).fill(2); }
    function renderSteps() {
      const i = patIndex(), pat = shownPattern();
      stepsBox.classList.toggle("preview", i !== CUSTOM && i !== 0);
      stepsBox.classList.toggle("off", i === 0);
      cells.forEach((c, n) => { c.dataset.l = pat[n]; c.setAttribute("aria-label", `Step ${n + 1}: ${["closed", "half", "open"][pat[n]]}`); });
      stepsBox.title = i === CUSTOM ? "Click or drag to draw. Right-click closes a step." : "Click a step to edit a copy as the Custom pattern.";
    }
    apf.on("chop_pattern", renderSteps);
    let stepsPost = 0;
    function postSteps() { clearTimeout(stepsPost); stepsPost = setTimeout(() => apf.plugin.post("steps", custom).catch((e) => apf.log("steps:", String(e))), 25); }
    async function refreshSteps() {
      try { patterns = await apf.plugin.get("patterns"); custom = await apf.plugin.get("steps"); } catch (e) { apf.log("patterns:", String(e)); }
      renderSteps();
    }
    await refreshSteps();
    committed = snapshot();

    let paint = null;
    const stepAt = (e) => { const el2 = document.elementFromPoint(e.clientX, e.clientY); return el2 && el2.closest ? el2.closest(".step") : null; };
    stepsBox.addEventListener("pointerdown", (e) => {
      const c = e.target.closest(".step"); if (!c) return;
      e.preventDefault(); stepsBox.setPointerCapture(e.pointerId);
      const from = custom.slice(), fromPattern = patParam.value;
      const wasCustom = patIndex() === CUSTOM;
      if (!wasCustom) { custom = shownPattern().slice(); apf.change("chop_pattern", CUSTOM / patParam.steps); }
      const i = +c.dataset.i;
      const level = e.button === 2 ? 0 : (custom[i] + 2) % 3;   // 2 -> 1 -> 0 -> 2
      paint = { level, from, fromPattern, wasCustom };
      setStep(i, level);
    });
    stepsBox.addEventListener("pointermove", (e) => {
      if (!paint) return;
      const c = stepAt(e); if (c) setStep(+c.dataset.i, paint.level);
    });
    const endPaint = () => {
      if (!paint) return;
      const { from, fromPattern, wasCustom } = paint, to = custom.slice(); paint = null;
      if (from.join() === to.join() && wasCustom) return;
      push({
        undo: () => { custom = from.slice(); postSteps(); renderSteps(); if (!wasCustom) apf.change("chop_pattern", fromPattern); },
        redo: () => { custom = to.slice(); postSteps(); if (!wasCustom) apf.change("chop_pattern", CUSTOM / patParam.steps); renderSteps(); },
      });
    };
    stepsBox.addEventListener("pointerup", endPaint);
    stepsBox.addEventListener("pointercancel", endPaint);
    stepsBox.addEventListener("contextmenu", (e) => e.preventDefault());
    function setStep(i, level) { if (custom[i] === level) { renderSteps(); return; } custom[i] = level; renderSteps(); postSteps(); }
    stepsBox.addEventListener("pointerenter", () => hint("Chop: click or drag to draw steps (open, half, closed). Right-click closes a step."));
    stepsBox.addEventListener("pointerleave", () => hint(""));

    /* ── A/B compare ─────────────────────────────────────────────────────────────── */
    const ab = { active: "A", A: null, B: null };
    const abA = $("#ab-a"), abB = $("#ab-b");
    const showAb = () => { abA.classList.toggle("on", ab.active === "A"); abB.classList.toggle("on", ab.active === "B"); abA.setAttribute("aria-pressed", ab.active === "A"); abB.setAttribute("aria-pressed", ab.active === "B"); };
    let abTimer = 0, abPending = false;
    function scheduleAb() { abPending = true; clearTimeout(abTimer); abTimer = setTimeout(saveAb, 400); }
    function saveAb() {
      abPending = false;
      ab[ab.active] = snapshot();
      apf.plugin.post("ab", { active: ab.active, A: ab.A, B: ab.B }).catch((e) => apf.log("ab:", String(e)));
    }
    async function loadAb() {
      try {
        const v = await apf.plugin.get("ab");
        if (v && typeof v === "object" && !abPending) { ab.active = v.active === "B" ? "B" : "A"; ab.A = v.A || null; ab.B = v.B || null; }
      } catch (e) { /* none yet */ }
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
      const b = other === "A" ? abA : abB; b.animate([{ background: "rgba(168,85,247,.7)" }, { background: "transparent" }], { duration: 500 });
    });
    abA.title = "Compare slot A"; abB.title = "Compare slot B";
    await loadAb();

    /* ── presets in the history; session changes from the host ─────────────────── */
    let lastPreset;
    apf.presets.on((cur2) => {
      const id = cur2 ? cur2.id : null;
      if (lastPreset === undefined) { lastPreset = id; return; }
      if (id === lastPreset) return;
      lastPreset = id;
      if (replaying) return;
      const before = committed;
      setTimeout(async () => {
        await refreshSteps();
        const after = snapshot();
        committed = after;
        H.u.push({ undo: () => applySnap(before), redo: () => applySnap(after) }); H.r.length = 0; syncHist();
        scheduleAb();
      }, 200);
    });
    apf.plugin.onChange(() => { refreshSteps(); loadAb(); });

    /* ── footer ──────────────────────────────────────────────────────────────────── */
    const fLat = $("#f-lat"), fSr = $("#f-sr"), fTr = $("#f-tr");
    let fText = "";
    function footer(sr, lat, playing) {
      const t = `${sr}|${lat}|${playing}`;
      if (t === fText) return; fText = t;
      fSr.textContent = sr ? `${(sr / 1000).toFixed(sr % 1000 ? 1 : 0)} kHz` : "— kHz";
      fLat.textContent = sr ? `Latency ${((lat / sr) * 1000).toFixed(1)} ms · ${Math.round(lat)} smp` : "Latency —";
      fTr.innerHTML = `<i class="dot${playing ? " on" : ""}"></i><b>${playing ? "Playing" : "Stopped"}</b>${playing ? "" : " · chop free-runs"}`;
    }

    // speed readout beside the analyzer
    const speedV = $(".speed-v");
    apf.on("pitch", () => { const st = plainNow("pitch"); speedV.textContent = `${(Math.pow(2, st / 12) * 100).toFixed(1)}%`; });

    /* ── the analyzer ────────────────────────────────────────────────────────────── */
    const BANDS = 160, HEAD = 16;
    const FLO = 20, FHI = 20000, LOGSPAN = Math.log10(FHI / FLO);
    const bandHz = (i) => FLO * Math.pow(FHI / FLO, (i + 0.5) / BANDS);
    const bandsIn = new Float32Array(BANDS).fill(-120), bandsOut = new Float32Array(BANDS).fill(-120);
    const showIn = new Float32Array(BANDS).fill(-120), showOut = new Float32Array(BANDS).fill(-120), hold = new Float32Array(BANDS).fill(-120), holdAge = new Float32Array(BANDS);
    const meters = { sr: 0, lat: 0, playing: 0, step: 255, inPk: [0, 0], outPk: [0, 0], inRms: [0, 0], outRms: [0, 0], gr: 1, lastData: 0 };
    let tilt = 3;
    const tiltBtn = $("#tilt");
    tiltBtn.addEventListener("click", () => { tilt = tilt === 3 ? 0 : tilt === 0 ? 4.5 : 3; tiltBtn.textContent = tilt ? `${tilt} dB/oct` : "Flat"; tiltBtn.classList.toggle("on", tilt !== 3); hold.fill(-120); });

    function ingest(buf) {
      if (buf.byteLength < (HEAD + 2 * BANDS) * 4) return false;
      const f = new Float32Array(buf);
      meters.inPk[0] = f[0]; meters.inPk[1] = f[1]; meters.outPk[0] = f[2]; meters.outPk[1] = f[3];
      meters.inRms[0] = f[4]; meters.inRms[1] = f[5]; meters.outRms[0] = f[6]; meters.outRms[1] = f[7];
      meters.gr = f[8] > 0 ? f[8] : 1; meters.step = f[9]; meters.sr = f[10]; meters.lat = f[11]; meters.playing = f[12];
      bandsIn.set(f.subarray(HEAD, HEAD + BANDS)); bandsOut.set(f.subarray(HEAD + BANDS, HEAD + 2 * BANDS));
      meters.lastData = performance.now();
      return f[0] > 1e-5 || f[1] > 1e-5 || f[2] > 1e-5 || f[3] > 1e-5 || f[12] > 0;
    }
    apf.poller(async () => ingest(await apf.plugin.raw("frame")), { fast: 33, slow: 100, idle: 250 });

    // biquad responses, the same RBJ forms as the DSP (crates/nodaw-dsp/src/filter.rs)
    const norm = (b0, b1, b2, a0, a1, a2) => [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0];
    function lowpassC(fc, q, sr) {
      const w = (2 * Math.PI * clamp(fc, 10, sr * 0.49)) / sr, s = Math.sin(w), c = Math.cos(w), a = s / (2 * q);
      return norm((1 - c) / 2, 1 - c, (1 - c) / 2, 1 + a, -2 * c, 1 - a);
    }
    function lowShelfC(fc, gdb, sr) {
      const A = Math.pow(10, gdb / 40), w = (2 * Math.PI * clamp(fc, 10, sr * 0.49)) / sr, s = Math.sin(w), c = Math.cos(w);
      const al = (s / 2) * Math.SQRT2, sq = 2 * Math.sqrt(A) * al;
      return norm(A * (A + 1 - (A - 1) * c + sq), 2 * A * (A - 1 - (A + 1) * c), A * (A + 1 - (A - 1) * c - sq), A + 1 + (A - 1) * c + sq, -2 * (A - 1 + (A + 1) * c), A + 1 + (A - 1) * c - sq);
    }
    function magDb(c, f, sr) {
      const w = (2 * Math.PI * f) / sr, c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
      const nr = c[0] + c[1] * c1 + c[2] * c2, ni = -(c[1] * s1 + c[2] * s2), dr = 1 + c[3] * c1 + c[4] * c2, di = -(c[3] * s1 + c[4] * s2);
      return 10 * Math.log10(Math.max(1e-12, (nr * nr + ni * ni) / (dr * dr + di * di)));
    }

    const spec = $("#spec"), sctx = spec.getContext("2d");
    const L = { w: 1, h: 1, dpr: 1, l: 0, r: 0, t: 0, b: 0, pw: 1, ph: 1 };
    const DB_TOP = 6, DB_BOT = -90, EQ_RANGE = 18;
    const fx = (f) => L.l + (Math.log10(f / FLO) / LOGSPAN) * L.pw;
    const fOfX = (x) => FLO * Math.pow(10, ((x - L.l) / L.pw) * LOGSPAN);
    const dy = (db) => L.t + ((DB_TOP - db) / (DB_TOP - DB_BOT)) * L.ph;
    const ey = (db) => L.t + L.ph * (0.5 - db / (2 * EQ_RANGE));
    const tiltOf = (f) => tilt * Math.log2(f / 1000);

    function sizeCanvas(c, ctx2, box) {
      const dpr = window.devicePixelRatio || 1, r = (box || c).getBoundingClientRect();
      const w = Math.max(2, Math.round(r.width * dpr)), h = Math.max(2, Math.round(r.height * dpr));
      if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
      ctx2.setTransform(dpr, 0, 0, dpr, 0, 0);
      return { w: r.width, h: r.height, dpr };
    }
    const mcv = $("#meters"), mctx = mcv.getContext("2d");
    let dirtyLayout = true;
    new ResizeObserver(() => { dirtyLayout = true; }).observe($("#display"));
    new ResizeObserver(() => { dirtyLayout = true; }).observe($("aside.meters"));

    const mouse = { x: -1, y: -1, in: false };
    let node = null, hoverNode = null;       // dragged / hovered node: "lowpass" | "bass"
    function nodePos() {
      const sr = meters.sr || 48000, lp = plainNow("lowpass"), bs = plainNow("bass");
      const lpC = lp >= 19900 ? [1, 0, 0, 0, 0] : lowpassC(lp, 0.707, sr);
      const shC = Math.abs(bs) < 0.01 ? [1, 0, 0, 0, 0] : lowShelfC(150, bs, sr);
      const total = (f) => (lp >= 19900 ? 0 : magDb(lpC, f, sr)) + (Math.abs(bs) < 0.01 ? 0 : magDb(shC, f, sr));
      return {
        lp: { x: fx(lp), y: ey(clamp(total(lp), -EQ_RANGE, EQ_RANGE)), f: lp },
        bs: { x: fx(150), y: ey(clamp(total(150), -EQ_RANGE, EQ_RANGE)), f: 150 },
        total, lpC, shC,
      };
    }

    function drawSpectrum(dt) {
      const ctx = sctx, R2 = rem();
      if (dirtyLayout) {
        const s = sizeCanvas(spec, ctx); L.w = s.w; L.h = s.h; L.dpr = s.dpr;
        L.l = 2.7 * R2; L.r = 2.7 * R2; L.t = 2.1 * R2; L.b = 1.5 * R2;
        L.pw = Math.max(10, L.w - L.l - L.r); L.ph = Math.max(10, L.h - L.t - L.b);
        dirtyLayout = false;
      }
      const w = L.w, h = L.h;
      ctx.clearRect(0, 0, w, h);
      // plate
      const bg = ctx.createLinearGradient(0, L.t, 0, L.t + L.ph);
      bg.addColorStop(0, "#0b0a12"); bg.addColorStop(1, "#060609");
      ctx.fillStyle = bg; ctx.fillRect(L.l, L.t, L.pw, L.ph);

      // grid
      ctx.lineWidth = 1; ctx.font = `500 ${(0.62 * R2).toFixed(1)}px "JetBrains Mono", monospace`; ctx.textBaseline = "middle";
      for (let db = -84; db <= 0; db += 12) {
        const y = Math.round(dy(db)) + 0.5;
        ctx.strokeStyle = db === 0 ? "rgba(255,255,255,.10)" : "rgba(255,255,255,.045)";
        ctx.beginPath(); ctx.moveTo(L.l, y); ctx.lineTo(L.l + L.pw, y); ctx.stroke();
        ctx.fillStyle = "#5d5a69"; ctx.textAlign = "right"; ctx.fillText(sign(String(db)), L.l - 0.5 * R2, y);
      }
      for (const db of [12, 0, -12]) {
        ctx.fillStyle = "#6c5f94"; ctx.textAlign = "left"; ctx.fillText(sign((db > 0 ? "+" : "") + db), L.l + L.pw + 0.5 * R2, ey(db));
      }
      ctx.strokeStyle = "rgba(var(--accent-rgb),.25)";
      ctx.strokeStyle = "rgba(168,85,247,.22)"; ctx.setLineDash([2, 4]);
      ctx.beginPath(); ctx.moveTo(L.l, Math.round(ey(0)) + 0.5); ctx.lineTo(L.l + L.pw, Math.round(ey(0)) + 0.5); ctx.stroke(); ctx.setLineDash([]);
      ctx.textAlign = "center"; ctx.textBaseline = "top";
      for (const f of [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
        const x = Math.round(fx(f)) + 0.5;
        ctx.strokeStyle = [100, 1000, 10000].includes(f) ? "rgba(255,255,255,.09)" : "rgba(255,255,255,.04)";
        ctx.beginPath(); ctx.moveTo(x, L.t); ctx.lineTo(x, L.t + L.ph); ctx.stroke();
        ctx.fillStyle = "#5d5a69";
        ctx.fillText(f >= 1000 ? `${f / 1000}k` : String(f), clamp(x, L.l + 1.0 * R2, L.l + L.pw - 0.8 * R2), L.t + L.ph + 0.45 * R2);
      }

      // ballistics (the data arrives about every 33 ms; this runs at the display rate)
      const stale = performance.now() - meters.lastData > 700;
      const aUp = 1 - Math.exp(-dt / 0.012), aDn = 1 - Math.exp(-dt / (stale ? 0.12 : 0.30));
      for (let i = 0; i < BANDS; i++) {
        const ti = stale ? -120 : bandsIn[i], to = stale ? -120 : bandsOut[i];
        showIn[i] += (ti - showIn[i]) * (ti > showIn[i] ? aUp : aDn);
        showOut[i] += (to - showOut[i]) * (to > showOut[i] ? aUp : aDn);
        if (showOut[i] >= hold[i]) { hold[i] = showOut[i]; holdAge[i] = 0; }
        else { holdAge[i] += dt; if (holdAge[i] > 0.9) hold[i] -= 14 * dt; }
      }

      ctx.save();
      ctx.beginPath(); ctx.rect(L.l, L.t, L.pw, L.ph); ctx.clip();
      const curve = (arr, close) => {
        ctx.beginPath();
        let px = 0, py = 0;
        for (let i = 0; i < BANDS; i++) {
          const f = bandHz(i), x = fx(f), y = dy(arr[i] + tiltOf(f));
          if (i === 0) { ctx.moveTo(L.l, y); ctx.lineTo(x, y); }
          else { const mx = (px + x) / 2, my = (py + y) / 2; ctx.quadraticCurveTo(px, py, mx, my); }
          px = x; py = y;
        }
        ctx.lineTo(px, py); ctx.lineTo(L.l + L.pw, py);
        if (close) { ctx.lineTo(L.l + L.pw, L.t + L.ph); ctx.lineTo(L.l, L.t + L.ph); ctx.closePath(); }
      };
      // input
      curve(showIn, true);
      const gi = ctx.createLinearGradient(0, L.t, 0, L.t + L.ph);
      gi.addColorStop(0, "rgba(150,156,180,.20)"); gi.addColorStop(1, "rgba(150,156,180,.02)");
      ctx.fillStyle = gi; ctx.fill();
      curve(showIn, false); ctx.strokeStyle = "rgba(170,176,200,.55)"; ctx.lineWidth = 1; ctx.stroke();
      // output
      curve(showOut, true);
      const go = ctx.createLinearGradient(0, L.t, 0, L.t + L.ph);
      go.addColorStop(0, "rgba(201,167,255,.62)"); go.addColorStop(0.45, "rgba(168,85,247,.34)"); go.addColorStop(1, "rgba(91,33,168,.04)");
      ctx.fillStyle = go; ctx.fill();
      ctx.shadowColor = "rgba(168,85,247,.9)"; ctx.shadowBlur = 0.7 * R2;
      curve(showOut, false); ctx.strokeStyle = "rgb(214,190,255)"; ctx.lineWidth = 1.6; ctx.stroke();
      ctx.shadowBlur = 0;
      // peak hold
      ctx.beginPath();
      for (let i = 0; i < BANDS; i++) { const f = bandHz(i), x = fx(f), y = dy(hold[i] + tiltOf(f)); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); }
      ctx.strokeStyle = "rgba(255,255,255,.18)"; ctx.lineWidth = 1; ctx.stroke();

      // tone curve
      const np = nodePos();
      const pts = [];
      for (let i = 0; i <= 220; i++) { const f = FLO * Math.pow(FHI / FLO, i / 220); pts.push([fx(f), ey(clamp(np.total(f), -EQ_RANGE, EQ_RANGE))]); }
      ctx.beginPath(); pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
      ctx.shadowColor = "rgba(255,255,255,.55)"; ctx.shadowBlur = 0.5 * R2;
      ctx.strokeStyle = "rgba(255,255,255,.92)"; ctx.lineWidth = 1.5; ctx.stroke(); ctx.shadowBlur = 0;
      ctx.lineTo(L.l + L.pw, ey(0)); ctx.lineTo(L.l, ey(0)); ctx.closePath();
      ctx.fillStyle = "rgba(255,255,255,.045)"; ctx.fill();
      ctx.restore();

      // nodes
      for (const [id, n, label] of [["lowpass", np.lp, `LP ${P("lowpass").text}`], ["bass", np.bs, `WARMTH ${sign(P("bass").text)}`]]) {
        const hot = node === id || (!node && hoverNode === id), r0 = (hot ? 0.52 : 0.42) * R2;
        const x = clamp(n.x, L.l + r0, L.l + L.pw - r0), y = clamp(n.y, L.t + r0, L.t + L.ph - r0);
        const g = ctx.createRadialGradient(x, y, 0, x, y, r0 * 2.6);
        g.addColorStop(0, `rgba(201,167,255,${hot ? 0.55 : 0.28})`); g.addColorStop(1, "rgba(168,85,247,0)");
        ctx.fillStyle = g; ctx.beginPath(); ctx.arc(x, y, r0 * 2.6, 0, 7); ctx.fill();
        ctx.beginPath(); ctx.arc(x, y, r0, 0, 7); ctx.fillStyle = hot ? "#fff" : "#0a0a0f"; ctx.fill();
        ctx.lineWidth = 2; ctx.strokeStyle = hot ? "rgb(168,85,247)" : "rgb(230,214,255)"; ctx.stroke();
        if (hot) {
          ctx.font = `600 ${(0.64 * R2).toFixed(1)}px "JetBrains Mono", monospace`; ctx.textAlign = x > L.l + L.pw * 0.75 ? "right" : "left"; ctx.textBaseline = "middle";
          const tx = x + (ctx.textAlign === "left" ? 1 : -1) * (r0 + 0.5 * R2), tw = ctx.measureText(label).width;
          ctx.fillStyle = "rgba(6,6,10,.85)";
          ctx.fillRect(ctx.textAlign === "left" ? tx - 0.35 * R2 : tx - tw - 0.35 * R2, y - 0.62 * R2, tw + 0.7 * R2, 1.24 * R2);
          ctx.fillStyle = "#fff"; ctx.fillText(label, tx, y);
        }
      }

      // cursor read-out
      if (mouse.in && mouse.x >= L.l && mouse.x <= L.l + L.pw && !node) {
        const f = fOfX(mouse.x), i = clamp(Math.round((Math.log(f / FLO) / Math.log(FHI / FLO)) * BANDS - 0.5), 0, BANDS - 1);
        ctx.strokeStyle = "rgba(255,255,255,.22)"; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(Math.round(mouse.x) + 0.5, L.t); ctx.lineTo(Math.round(mouse.x) + 0.5, L.t + L.ph); ctx.stroke();
        const names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        const midi = 69 + 12 * Math.log2(f / 440), mr = Math.round(midi), cents = Math.round((midi - mr) * 100);
        const note = `${names[((mr % 12) + 12) % 12]}${Math.floor(mr / 12) - 1}${cents >= 0 ? "+" : MINUS}${Math.abs(cents)}c`;
        const text = `${f >= 1000 ? (f / 1000).toFixed(2) + " kHz" : f.toFixed(0) + " Hz"}  ${note}  out ${sign(Math.max(-99.9, showOut[i]).toFixed(1))} dB`;
        ctx.font = `500 ${(0.64 * R2).toFixed(1)}px "JetBrains Mono", monospace`; const tw = ctx.measureText(text).width;
        const bx = clamp(mouse.x + 0.8 * R2, L.l, L.l + L.pw - tw - 0.8 * R2), by = L.t + L.ph - 1.7 * R2;
        ctx.fillStyle = "rgba(6,6,10,.88)"; ctx.fillRect(bx, by, tw + 0.8 * R2, 1.3 * R2);
        ctx.fillStyle = "#d9d4e8"; ctx.textAlign = "left"; ctx.textBaseline = "middle"; ctx.fillText(text, bx + 0.4 * R2, by + 0.65 * R2);
      }
    }

    // dragging the nodes
    const hit = (x, y) => {
      const np = nodePos(), r = 0.9 * rem();
      const d = (n) => Math.hypot(n.x - x, n.y - y);
      const cand = [["lowpass", d(np.lp)], ["bass", d(np.bs)]].filter((c) => c[1] < r).sort((a, b) => a[1] - b[1]);
      return cand.length ? cand[0][0] : null;
    };
    let nodeDrag = null;
    spec.addEventListener("pointermove", (e) => {
      const r = spec.getBoundingClientRect(); mouse.x = e.clientX - r.left; mouse.y = e.clientY - r.top; mouse.in = true;
      if (nodeDrag) {
        const key = nodeDrag.key, fine = e.shiftKey ? 0.1 : 1;
        if (key === "lowpass") {
          nodeDrag.lf += ((mouse.x - nodeDrag.px) / L.pw) * LOGSPAN * fine;
          apf.set(key, normOf(key, clamp(FLO * Math.pow(10, nodeDrag.lf), 800, 20000)));
        } else {
          nodeDrag.db -= (mouse.y - nodeDrag.py) * ((2 * EQ_RANGE) / L.ph) * fine;
          nodeDrag.db = clamp(nodeDrag.db, R.bass.min, R.bass.max);
          apf.set(key, normOf(key, nodeDrag.db));
        }
        nodeDrag.px = mouse.x; nodeDrag.py = mouse.y;
      } else { hoverNode = hit(mouse.x, mouse.y); spec.style.cursor = hoverNode ? (hoverNode === "bass" ? "ns-resize" : "ew-resize") : "crosshair"; }
    });
    spec.addEventListener("pointerleave", () => { mouse.in = false; hoverNode = null; });
    spec.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      const r = spec.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top, id = hit(x, y);
      if (!id) return;
      spec.setPointerCapture(e.pointerId); node = id;
      nodeDrag = { key: id, from: P(id).value, px: x, py: y, lf: Math.log10(plainNow(id) / FLO), db: plainNow(id) };
      apf.begin(id); e.preventDefault();
    });
    const endNode = () => { if (!nodeDrag) return; apf.end(nodeDrag.key); recordParam(nodeDrag.key, nodeDrag.from, P(nodeDrag.key).value); nodeDrag = null; node = null; };
    spec.addEventListener("pointerup", endNode); spec.addEventListener("pointercancel", endNode);
    spec.addEventListener("dblclick", (e) => {
      const r = spec.getBoundingClientRect(), id = hit(e.clientX - r.left, e.clientY - r.top);
      if (id) { const from = P(id).value; apf.change(id, P(id).default); recordParam(id, from, P(id).default); }
    });
    spec.addEventListener("pointerenter", () => hint("Drag the nodes: low-pass (left/right) and warmth (up/down). Shift for fine, double-click to reset."));
    spec.addEventListener("pointerleave", () => hint(""));

    /* ── meters ──────────────────────────────────────────────────────────────────── */
    const M = {
      inPk: [-120, -120], outPk: [-120, -120], inRms: [-120, -120], outRms: [-120, -120],
      inHold: [-120, -120], outHold: [-120, -120], inAge: [0, 0], outAge: [0, 0], inClip: false, outClip: false, gr: 0, grShown: 0, outMax: -120, inMax: -120,
    };
    const lin2db = (v) => (v > 1e-6 ? 20 * Math.log10(v) : -120);
    mcv.addEventListener("click", () => { M.inClip = M.outClip = false; M.inMax = M.outMax = -120; M.inHold = [-120, -120]; M.outHold = [-120, -120]; });
    const MET_TOP = 3, MET_BOT = -60;

    function drawMeters(dt) {
      const ctx = mctx, R2 = rem();
      const s = sizeCanvas(mcv, ctx), w = s.w, h = s.h;
      ctx.clearRect(0, 0, w, h);
      const stale = performance.now() - meters.lastData > 700;
      const upd = (shown, tgt, upA, dnDb) => (tgt > shown ? tgt : Math.max(tgt, shown - dnDb * dt));
      for (let c = 0; c < 2; c++) {
        const ip = stale ? -120 : lin2db(meters.inPk[c]), op = stale ? -120 : lin2db(meters.outPk[c]);
        M.inPk[c] = upd(M.inPk[c], ip, 1, 55); M.outPk[c] = upd(M.outPk[c], op, 1, 55);
        const ir = stale ? -120 : lin2db(meters.inRms[c]), orr = stale ? -120 : lin2db(meters.outRms[c]);
        M.inRms[c] += (ir - M.inRms[c]) * (1 - Math.exp(-dt / (ir > M.inRms[c] ? 0.04 : 0.28)));
        M.outRms[c] += (orr - M.outRms[c]) * (1 - Math.exp(-dt / (orr > M.outRms[c] ? 0.04 : 0.28)));
        for (const [hl, pk, age] of [[M.inHold, M.inPk, M.inAge], [M.outHold, M.outPk, M.outAge]]) {
          if (pk[c] >= hl[c]) { hl[c] = pk[c]; age[c] = 0; } else { age[c] += dt; if (age[c] > 1.6) hl[c] -= 18 * dt; }
        }
        if (ip > -0.1) M.inClip = true; if (op > -0.1) M.outClip = true;
        M.inMax = Math.max(M.inMax, ip); M.outMax = Math.max(M.outMax, op);
      }
      const gdb = stale ? 0 : Math.min(0, lin2db(meters.gr));
      M.gr = gdb; M.grShown = gdb < M.grShown ? gdb : M.grShown + (gdb - M.grShown) * (1 - Math.exp(-dt / 0.35));

      const barW = 0.72 * R2, gap = 0.14 * R2, pad = 0.75 * R2, top = 2.2 * R2, bot = h - 2.9 * R2, ph = Math.max(10, bot - top);
      const y = (db) => top + ph * (1 - clamp01((db - MET_BOT) / (MET_TOP - MET_BOT)));
      const grp = (x0) => [x0, x0 + barW + gap];
      const inX = grp(pad), scaleX = inX[1] + barW + 0.5 * R2, outX = grp(scaleX + 1.35 * R2 + 0.35 * R2), grX = outX[1] + barW + 0.65 * R2;
      const font = (px, wt = 500) => `${wt} ${(px * R2).toFixed(1)}px "JetBrains Mono", monospace`;
      ctx.textBaseline = "middle";

      // labels
      ctx.font = font(0.62, 600); ctx.fillStyle = "#8b8797"; ctx.textAlign = "center";
      ctx.fillText("IN", inX[0] + (barW * 2 + gap) / 2, 1.0 * R2);
      ctx.fillText("OUT", outX[0] + (barW * 2 + gap) / 2, 1.0 * R2);
      ctx.fillText("GR", grX + barW / 2, 1.0 * R2);

      // the lit bar: colour by level
      const grad = ctx.createLinearGradient(0, y(MET_TOP), 0, y(MET_BOT));
      grad.addColorStop(0, "#ff4f6d"); grad.addColorStop(0.07, "#ffbf4a"); grad.addColorStop(0.2, "#c9a7ff"); grad.addColorStop(0.55, "#a855f7"); grad.addColorStop(1, "#4c1d95");
      const bar = (x, rms, pk, hold) => {
        ctx.fillStyle = "#08080c"; ctx.fillRect(x, top, barW, ph);
        ctx.strokeStyle = "rgba(255,255,255,.06)"; ctx.lineWidth = 1; ctx.strokeRect(x + 0.5, top + 0.5, barW - 1, ph - 1);
        ctx.save(); ctx.beginPath(); ctx.rect(x + 1, top + 1, barW - 2, ph - 2); ctx.clip();
        ctx.globalAlpha = 0.5; ctx.fillStyle = grad; ctx.fillRect(x + 1, y(rms), barW - 2, bot - y(rms));
        ctx.globalAlpha = 1; ctx.fillRect(x + barW * 0.28, y(pk), barW * 0.44, bot - y(pk));
        ctx.restore();
        ctx.fillStyle = hold > -0.5 ? "#ff4f6d" : "#fff"; ctx.fillRect(x + 1, Math.round(y(hold)) - 1, barW - 2, 2);
      };
      for (let c = 0; c < 2; c++) { bar(inX[c], M.inRms[c], M.inPk[c], M.inHold[c]); bar(outX[c], M.outRms[c], M.outPk[c], M.outHold[c]); }

      // scale
      ctx.font = font(0.58); ctx.textAlign = "center"; ctx.fillStyle = "#5d5a69";
      for (const db of [0, -6, -12, -18, -24, -36, -48, -60]) {
        const yy = y(db); ctx.fillText(sign(String(db)), scaleX + 0.7 * R2, yy);
        ctx.strokeStyle = "rgba(255,255,255,.05)"; ctx.beginPath(); ctx.moveTo(inX[0], Math.round(yy) + 0.5); ctx.lineTo(inX[1] + barW, Math.round(yy) + 0.5); ctx.stroke();
        ctx.beginPath(); ctx.moveTo(outX[0], Math.round(yy) + 0.5); ctx.lineTo(outX[1] + barW, Math.round(yy) + 0.5); ctx.stroke();
      }
      // clip LEDs
      for (const [x0, clip] of [[inX[0], M.inClip], [outX[0], M.outClip]]) {
        ctx.fillStyle = clip ? "#ff4f6d" : "#1a1a22"; if (clip) { ctx.shadowColor = "#ff4f6d"; ctx.shadowBlur = 8; }
        ctx.fillRect(x0, 1.5 * R2 + 0.15 * R2, barW * 2 + gap, 0.3 * R2); ctx.shadowBlur = 0;
      }
      // gain reduction (hangs from the top, 0..-12 dB)
      ctx.fillStyle = "#08080c"; ctx.fillRect(grX, top, barW, ph); ctx.strokeStyle = "rgba(255,255,255,.06)"; ctx.strokeRect(grX + 0.5, top + 0.5, barW - 1, ph - 1);
      const gh = ph * clamp01(-M.grShown / 12);
      const gg = ctx.createLinearGradient(0, top, 0, top + ph); gg.addColorStop(0, "#ffbf4a"); gg.addColorStop(1, "#ff4f6d");
      ctx.fillStyle = gg; ctx.fillRect(grX + 1, top + 1, barW - 2, Math.max(0, gh - 1));
      ctx.font = font(0.52); ctx.fillStyle = "#5d5a69"; ctx.textAlign = "left";
      for (const db of [0, -3, -6, -12]) ctx.fillText(sign(String(db)), grX + barW + 0.18 * R2, top + ph * (-db / 12) + (db === 0 ? 0.3 * R2 : 0));

      // read-outs
      const rd = (db) => (db <= -99 ? "−∞" : sign(db.toFixed(1)));
      ctx.font = font(0.66, 600); ctx.textAlign = "center";
      ctx.fillStyle = M.inMax > -0.1 ? "#ff4f6d" : "#d9d4e8"; ctx.fillText(rd(M.inMax), inX[0] + (barW * 2 + gap) / 2, bot + 1.15 * R2);
      ctx.fillStyle = M.outMax > -0.1 ? "#ff4f6d" : "#d9d4e8"; ctx.fillText(rd(M.outMax), outX[0] + (barW * 2 + gap) / 2, bot + 1.15 * R2);
      ctx.fillStyle = M.grShown < -0.05 ? "#ffbf4a" : "#6a6677"; ctx.fillText(M.grShown < -0.05 ? sign(M.grShown.toFixed(1)) : "0.0", grX + barW / 2, bot + 1.15 * R2);
      ctx.font = font(0.5); ctx.fillStyle = "#44414f";
      ctx.fillText("PEAK dBFS", (inX[0] + outX[1] + barW) / 2 - 0.1 * R2, bot + 2.15 * R2);
      ctx.fillText("click to clear", (inX[0] + outX[1] + barW) / 2 - 0.1 * R2, bot + 2.65 * R2);
    }

    /* ── the loop ────────────────────────────────────────────────────────────────── */
    let last = performance.now(), chopNow = -1;
    function loop(now) {
      const dt = Math.min(0.1, Math.max(0.001, (now - last) / 1000)); last = now;
      drawSpectrum(dt); drawMeters(dt);
      const st = performance.now() - meters.lastData > 700 || meters.step > 15 || patIndex() === 0 ? -1 : meters.step;
      if (st !== chopNow) { if (chopNow >= 0) cells[chopNow].classList.remove("now"); if (st >= 0) cells[st].classList.add("now"); chopNow = st; }
      footer(meters.sr, meters.lat, meters.playing > 0);
      requestAnimationFrame(loop);
    }
    requestAnimationFrame(loop);

    /* ── trial and license (state from the plugin: license.rs) ───────────────────── */
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
      $("#lic-bar").style.width = licensed ? "100%" : ended ? "100%" : `${Math.max(4, (100 * s.days_left) / (s.trial_days || 14))}%`;
      $("#lic-sub").textContent = licensed
        ? "Every feature, for good. Thank you for supporting independent tools."
        : ended
          ? (st === "tampered" ? "The license file was changed, so the trial has ended. " : "") + "NoDAW Screw keeps working, but the output drops out for half a second every 45 seconds. Activate to remove it. Your sessions and presets are safe."
          : "Every feature is unlocked during the trial. After it, the output drops out briefly every 45 seconds until you activate.";
      $("#lic-licensed").hidden = !licensed;
      $("#lic-who").textContent = s.email ? `Licensed to ${s.email}` : "Licensed";
      $("#lic-keyhint").textContent = s.key_hint ? `Key ${s.key_hint}` : "";
      $("#lic-fine").textContent = s.activation_ready === false ? "Activation opens when NoDAW Screw goes on sale." : "Your key is in the Gumroad receipt email.";
      licBtn.disabled = !!s.busy;
      licBtn.classList.toggle("busy", !!s.busy);
      licBtn.textContent = s.busy ? "Checking…" : "Activate";
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

    window.__screw = { snapshot, applySnap, undo, redo, history: H, ab, meters, lic }; // for tests
  });
})();
