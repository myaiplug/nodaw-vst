/* NoDAW Chronos Weaver: plugin UI on the NoDAW kit (ui-kit/kit.js).
 *
 * The 3D spectrum (frequency across, time into the screen, level up; drag to orbit) with
 * source B's spectrum on the front edge, the morph track (the knob and the live, LFO-moved
 * position), and the Morph, Freeze, Cloud, LFO and Master sections. Capture B is a parameter
 * that the button pulses; the engine reports when the snapshot is held.
 */
(() => {
  "use strict";
  const K = NoDAW, $ = K.$, $$ = K.$$;
  K.fitRoot(980, 620);
  K.shell({ name: "NoDAW Chronos Weaver", prod: "CHRONOS WEAVER", version: "v0.1" });

  K.knobs({
    "g-morph": [
      { k: "morph", label: "Morph A→B", hint: "Morph: from the live signal (A) to source B. Blend morphs the spectra; Cross puts B's spectral envelope on A." },
      { k: "weave", tiny: true, label: "Weave", hint: "Weave time: how far back the Past source looks." },
    ],
    "g-freeze": [{ k: "sustain", mini: true, label: "Sustain", hint: "How long a frozen spectrum lasts: 100% holds it forever." }],
    "g-cloud": [
      { k: "cloud", mini: true, label: "Cloud", hint: "Granular cloud: smears every bin in time, gates it by density and scatters its phase." },
      { k: "grain", tiny: true, label: "Grain", hint: "Grain time: how long each bin's smear lasts." },
      { k: "density", tiny: true, label: "Density", hint: "How many bins are alive at once. Low is sparse and sparkly." },
      { k: "scatter", tiny: true, label: "Scatter", hint: "Phase scatter: from focused to diffuse." },
    ],
    "g-lfo": [{ k: "lfo_depth", mini: true, label: "Depth", hint: "LFO depth on the morph, synced to the host tempo." }],
    "g-master": [
      { k: "out_gain", mini: true, bipolar: true, label: "Output", hint: "Output level of the weave (the wet signal)." },
      { k: "mix", mini: true, label: "Mix", hint: "Dry/wet. The dry is delayed to line up with the FFT." },
    ],
  });
  $("#g-morph .knob").classList.add("hero");
  $("#c-mode").append(K.choice("morph_mode", ["Blend", "Cross"]));
  $("#c-src").append(K.choice("source", ["Past", "Snapshot"]));
  $("#cap").append(K.pill("capture", "Capture B", "cap-p"));
  $("#frz-btn").append(K.pill("freeze", "Freeze", "frz-p"));
  $("#c-sync").append(K.select("lfo_sync", "sync-sel"));
  $("#c-shape").append(K.choice("lfo_shape", ["Sin", "Tri", "Saw"]));

  K.start({
    init: async (ctx) => {
      const { P, plainNow, on } = ctx;

      /* presets by family */
      const fams = $("#st-fams"), list = $("#st-list");
      let all = [], fam = null, current = null;
      function renderList() {
        list.textContent = "";
        for (const r of all.filter((x) => x.type === fam)) {
          const b = K.h("button", "wv"); b.type = "button"; b.textContent = r.name; b.title = r.description || "";
          b.classList.toggle("on", current === r.name);
          b.addEventListener("click", async () => { try { await apf.presets.load(r.id); } catch (e) { apf.log("weave:", String(e)); } });
          b.addEventListener("pointerenter", () => ctx.hint(r.description || r.name));
          b.addEventListener("pointerleave", () => ctx.hint(""));
          list.append(b);
        }
        $$("button", fams).forEach((t) => t.classList.toggle("on", t.dataset.fam === fam));
      }
      try {
        const r = await apf.presets.list();
        all = (r.presets || []).filter((p) => p.factory && p.type && p.type !== "Utility");
        const order = [];
        for (const p of all) if (!order.includes(p.type)) order.push(p.type);
        for (const f of order) { const t = K.h("button", "", f); t.type = "button"; t.dataset.fam = f; t.setAttribute("role", "tab"); t.addEventListener("click", () => { fam = f; renderList(); }); fams.append(t); }
        fam = order[0] || null;
      } catch (e) { apf.log("weaves:", String(e)); }
      apf.presets.on((p) => { current = p && p.name ? p.name : null; const hit = all.find((x) => x.name === current); if (hit) fam = hit.type; renderList(); });
      renderList();

      /* colourway (session state through the controller) */
      let look = "nebula";
      function applyLook(s) {
        look = s === "ice" ? "ice" : "nebula";
        document.documentElement.classList.remove("look-nebula", "look-ice");
        document.documentElement.classList.add(`look-${look}`);
        $$("#looks button").forEach((b) => b.classList.toggle("on", b.dataset.look === look));
        K.refreshDefs();
      }
      ctx.ext = {
        get: () => ({ skin: look }),
        set: (o) => { applyLook(o && o.skin); apf.plugin.post("skin", JSON.stringify(look)).catch((e) => apf.log("skin:", String(e))); },
        refresh: async () => { try { applyLook(await apf.plugin.get("skin")); } catch (e) { applyLook("nebula"); } },
      };
      $("#looks").addEventListener("click", (e) => {
        const b = e.target.closest("button"); if (!b) return;
        const from = ctx.ext.get(); ctx.ext.set({ skin: b.dataset.look }); ctx.recordExt(from, ctx.ext.get());
      });

      /* Capture B is a pulse: on, then off again once the engine has had time to capture */
      let capTimer = 0;
      apf.on("capture", (v) => { if (v >= 0.5) { clearTimeout(capTimer); capTimer = setTimeout(() => apf.change("capture", 0), 900); } });

      /* the 3D spectrum */
      const cv = $("#w3d"), c2 = cv.getContext("2d");
      const ROWS = 44, COLS = 96;
      const hist = Array.from({ length: ROWS }, () => new Float32Array(COLS).fill(-120));
      let head = 0, rowAcc = 0;
      const view = { yaw: -0.35, tilt: 0.55 };
      let drag = null;
      cv.addEventListener("pointerdown", (e) => { drag = { x: e.clientX, y: e.clientY, yaw: view.yaw, tilt: view.tilt }; cv.setPointerCapture(e.pointerId); });
      cv.addEventListener("pointermove", (e) => { if (!drag) return; view.yaw = K.clamp(drag.yaw + (e.clientX - drag.x) * 0.004, -0.9, 0.9); view.tilt = K.clamp(drag.tilt - (e.clientY - drag.y) * 0.004, 0.15, 1.1); });
      const endDrag = () => { drag = null; };
      cv.addEventListener("pointerup", endDrag); cv.addEventListener("pointercancel", endDrag);
      cv.addEventListener("dblclick", () => { view.yaw = -0.35; view.tilt = 0.55; });
      cv.addEventListener("pointerenter", () => ctx.hint("Drag to orbit the 3D spectrum; double-click to reset the view."));
      cv.addEventListener("pointerleave", () => ctx.hint(""));
      const rgbv = (n, a) => `rgba(${getComputedStyle(document.documentElement).getPropertyValue(n).trim()},${a})`;

      // the analyser's 3 dB/oct slope, so pink-ish material reads flat (as on every NoDAW display)
      const tiltDb = (x) => 3 * Math.log2((20 * Math.pow(1000, x)) / 1000);
      function sample(arr, n, out) {
        // arr: 160 log bands (20 Hz .. 20 kHz) -> n columns (max)
        const m = arr.length;
        for (let j = 0; j < n; j++) {
          const a = Math.floor((j / n) * m), b = Math.max(a + 1, Math.floor(((j + 1) / n) * m));
          let v = -120; for (let k = a; k < b; k++) v = Math.max(v, arr[k]);
          out[j] = v + tiltDb((j + 0.5) / n);
        }
      }
      function draw3d(dt, ex, stale) {
        const s = K.sizeCanvas(cv, c2), w = s.w, h = s.h, c = c2;
        c.clearRect(0, 0, w, h);
        const bg = c.createLinearGradient(0, 0, 0, h);
        bg.addColorStop(0, look === "ice" ? "#06121c" : "#0d0718"); bg.addColorStop(1, "#030207");
        c.fillStyle = bg; c.fillRect(0, 0, w, h);
        // a new row every ~45 ms
        rowAcc += dt;
        const outBands = ctx.bandsOut;
        if (rowAcc > 0.045 && !stale && outBands) { rowAcc = 0; head = (head + 1) % ROWS; sample(outBands, COLS, hist[head]); }
        const R2 = K.rem();
        const W = w * 0.86, D = h * 0.95, H = h * 0.5;
        const cx = w * 0.5, base = h * 0.84;
        const proj = (x, z, y) => {
          // x: 0..1 frequency, z: 0 (front) .. 1 (back), y: 0..1 level
          const xr = (x - 0.5) * W, zr = z * D;
          const X = xr * Math.cos(view.yaw) - zr * Math.sin(view.yaw) * 0.5;
          const Z = zr * Math.cos(view.yaw) + xr * Math.sin(view.yaw) * 0.3;
          const sc = 1 / (1 + Z / (D * 1.6));
          return [cx + X * sc, base - Z * view.tilt * 0.62 * sc - y * H * sc * (1.25 - view.tilt * 0.4)];
        };
        const lvl = (db) => K.clamp01((db + 90) / 84);
        // floor grid
        c.strokeStyle = rgbv("--accent-rgb", 0.08); c.lineWidth = 1;
        for (let i = 0; i <= 10; i++) { const [x0, y0] = proj(i / 10, 0, 0), [x1, y1] = proj(i / 10, 1, 0); c.beginPath(); c.moveTo(x0, y0); c.lineTo(x1, y1); c.stroke(); }
        for (let i = 0; i <= 6; i++) { const [x0, y0] = proj(0, i / 6, 0), [x1, y1] = proj(1, i / 6, 0); c.beginPath(); c.moveTo(x0, y0); c.lineTo(x1, y1); c.stroke(); }
        // rows back to front
        for (let r = ROWS - 1; r >= 0; r--) {
          const row = hist[(head - r + ROWS) % ROWS];
          const z = r / (ROWS - 1);
          const a = 0.85 * (1 - z) + 0.08;
          c.beginPath();
          const [sx, sy] = proj(0, z, 0); c.moveTo(sx, sy);
          for (let j = 0; j < COLS; j++) { const [x, y] = proj(j / (COLS - 1), z, lvl(row[j])); c.lineTo(x, y); }
          const [ex2, ey2] = proj(1, z, 0); c.lineTo(ex2, ey2); c.closePath();
          c.fillStyle = rgbv("--accent-well-rgb", 0.55 * (1 - z) + 0.25);
          c.fill();
          c.strokeStyle = r === 0 ? rgbv("--accent-hot-rgb", 0.95) : rgbv(r % 4 === 0 ? "--accent-2-rgb" : "--accent-rgb", a * 0.75);
          c.lineWidth = r === 0 ? 2 : 1;
          c.stroke();
        }
        // source B on the front edge
        const bdb = ex.slice(3, 35);
        if (bdb.length === 32 && bdb.some((v) => v > -110)) {
          c.beginPath();
          for (let j = 0; j < 32; j++) { const fx = (j + 0.5) / 32; const [x, y] = proj(fx, 0, lvl(bdb[j] + tiltDb(fx))); if (j) c.lineTo(x, y); else c.moveTo(x, y); }
          c.strokeStyle = rgbv("--b-rgb", 0.95); c.lineWidth = 1.6; c.setLineDash([4, 3]); c.stroke(); c.setLineDash([]);
        }
        // frequency labels
        c.font = `500 ${(0.6 * R2).toFixed(1)}px "JetBrains Mono", monospace`; c.textAlign = "center"; c.textBaseline = "top"; c.fillStyle = "#6b6585";
        for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) { const x = Math.log10(f / 20) / 3; const [px, py] = proj(x, 0, 0); c.fillText(f >= 1000 ? `${f / 1000}k` : String(f), px, py + 0.3 * R2); }
      }

      const meters = ctx.levelMeters($("#meters"), { gr: false });
      const frz = $("#frz"), bState = $("#b-state"), fill = $("#mt-fill"), live = $("#mt-live"), mtv = $("#mt-v"), capCell = $("#cap");
      ctx.onDraw((dt) => {
        const ex = ctx.extra || [], stale = ctx.stale();
        draw3d(dt, ex, stale);
        meters.draw(dt);
        const m = plainNow("morph") / 100, lm = stale || ex.length < 3 ? m : ex[0];
        fill.style.width = `${m * 100}%`; live.style.left = `${lm * 100}%`;
        mtv.textContent = `${Math.round(lm * 100)}%`;
        frz.hidden = !(on("freeze") || (!stale && ex[1] > 0.02));
        const snap = !stale && ex[2] >= 0.5;
        bState.textContent = ctx.idx("source") === 1 ? (snap ? "B · snapshot held" : "B · no snapshot: press Capture B") : `B · past ${P("weave").text}`;
        bState.classList.toggle("warn", ctx.idx("source") === 1 && !snap);
        capCell.classList.toggle("held", snap);
      });
    },
  });
})();
