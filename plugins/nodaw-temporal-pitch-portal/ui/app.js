/* NoDAW Temporal Pitch Portal: plugin UI on the NoDAW kit (ui-kit/kit.js).
 *
 * The prototype's portal, rebuilt as a plugin face: Warp Zones and ScrewAI Flavors (the factory
 * presets), the Dimensional Coordinates read from the engine, the animated portal (rings turn
 * with the time shift and warp with the pitch; the outer arc is the warp period), the
 * Dimensional History (a bubble per edit, click to return), a spectrum and the meters.
 */
(() => {
  "use strict";
  const K = NoDAW, $ = K.$, $$ = K.$$;
  K.fitRoot(960, 620);
  K.shell({ name: "NoDAW Temporal Pitch Portal", prod: "PITCH PORTAL", version: "v0.1" });

  K.knobs({
    "g-portal": [
      { k: "time", label: "Time Shift", hint: "Time shift: 50% half-time to 200% double-time, inside each sync period. Stretch keeps the pitch; Tape bends it." },
      { k: "pitch", bipolar: true, label: "Pitch Bend", hint: "Pitch bend in semitones, independent of time. Alt-drag snaps to whole semitones." },
      { k: "glide", tiny: true, label: "Glide", hint: "How long pitch changes take to arrive." },
    ],
    "g-drift": [
      { k: "drift", mini: true, label: "Depth", hint: "Drift: a slow, wandering micro pitch, in cents." },
      { k: "drift_rate", mini: true, label: "Rate", hint: "How fast the drift wanders." },
    ],
    "g-tone": [{ k: "tone", mini: true, bipolar: true, label: "Tilt", hint: "Tilt: left is darker (syrup), right is brighter (glass)." }],
    "g-echo": [
      { k: "echo_mix", mini: true, label: "Echo", hint: "Echo return of the shifted signal." },
      { k: "echo_time", tiny: true, label: "Time", hint: "Time between echoes." },
      { k: "echo_fb", tiny: true, label: "Feedback", hint: "How many echoes." },
    ],
    "g-master": [
      { k: "out_gain", mini: true, bipolar: true, label: "Output", hint: "Output level of the portal (the wet signal)." },
      { k: "mix", mini: true, label: "Mix", hint: "Dry/wet. The dry is delayed to line up with the portal." },
    ],
  });
  $("#g-portal .knob[data-key=\"pitch\"]").dataset.snap = "1";
  $$("#g-portal .knob").forEach((k, i) => { if (i < 2) k.classList.add("hero"); });
  $("#c-mode").append(K.choice("warp_mode", ["Stretch", "Tape"]));
  $("#c-sync").append(K.choice("warp_sync", ["1/4", "1/2", "1 BAR", "2 BAR"]));

  K.start({
    init: async (ctx) => {
      const { P, plainNow } = ctx;

      /* zones: the factory presets by family */
      const fams = $("#z-fams"), list = $("#z-list");
      let zones = [], fam = null, current = null;
      function renderList() {
        list.textContent = "";
        for (const r of zones.filter((x) => x.type === fam)) {
          const b = K.h("button", "zn"); b.type = "button"; b.textContent = r.name; b.title = r.description || "";
          b.classList.toggle("on", current === r.name);
          b.addEventListener("click", async () => { try { await apf.presets.load(r.id); } catch (e) { apf.log("zone:", String(e)); } });
          b.addEventListener("pointerenter", () => ctx.hint(r.description || r.name));
          b.addEventListener("pointerleave", () => ctx.hint(""));
          list.append(b);
        }
        $$("button", fams).forEach((t) => t.classList.toggle("on", t.dataset.fam === fam));
      }
      try {
        const r = await apf.presets.list();
        zones = (r.presets || []).filter((p) => p.factory && p.type && p.type !== "Utility");
        const order = [];
        for (const p of zones) if (!order.includes(p.type)) order.push(p.type);
        for (const f of order) { const t = K.h("button", "", f); t.type = "button"; t.dataset.fam = f; t.setAttribute("role", "tab"); t.addEventListener("click", () => { fam = f; renderList(); }); fams.append(t); }
        fam = order[0] || null;
      } catch (e) { apf.log("zones:", String(e)); }
      apf.presets.on((p) => { current = p && p.name ? p.name : null; const hit = zones.find((x) => x.name === current); if (hit) fam = hit.type; renderList(); });
      renderList();

      /* faceplate (session state through the controller) */
      let look = "dark";
      function applyLook(s) {
        look = s === "light" ? "light" : "dark";
        document.documentElement.classList.remove("look-dark", "look-light");
        document.documentElement.classList.add(`look-${look}`);
        $$("#looks button").forEach((b) => b.classList.toggle("on", b.dataset.look === look));
        K.refreshDefs();
      }
      ctx.ext = {
        get: () => ({ skin: look }),
        set: (o) => { applyLook(o && o.skin); apf.plugin.post("skin", JSON.stringify(look)).catch((e) => apf.log("skin:", String(e))); },
        refresh: async () => { try { applyLook(await apf.plugin.get("skin")); } catch (e) { applyLook("dark"); } },
      };
      $("#looks").addEventListener("click", (e) => {
        const b = e.target.closest("button"); if (!b) return;
        const from = ctx.ext.get(); ctx.ext.set({ skin: b.dataset.look }); ctx.recordExt(from, ctx.ext.get());
      });

      /* dimensional history: a bubble per committed edit of time and pitch (20 kept) */
      const hist = $("#hist");
      const states = [];
      let lastKey = "";
      function snapState() {
        const t = Math.round(plainNow("time")), p = Math.round(plainNow("pitch") * 10) / 10;
        const key = `${t}|${p}`;
        if (key === lastKey) return;
        lastKey = key;
        states.push({ t, p, raw: { time: plainNow("time"), pitch: plainNow("pitch") } });
        if (states.length > 20) states.shift();
        renderHist();
      }
      function renderHist() {
        hist.textContent = "";
        states.forEach((s, i) => {
          const b = K.h("button", "hb"); b.type = "button";
          b.innerHTML = `<span>${s.t}%</span><span>${s.p > 0 ? "+" : ""}${K.sign(s.p.toFixed(1))}</span>`;
          b.classList.toggle("now", i === states.length - 1);
          b.title = `Back to ${s.t}% · ${s.p} st`;
          b.addEventListener("click", () => { ctx.changeMany(s.raw); });
          if (i) hist.append(K.h("i", "hl"));
          hist.append(b);
        });
      }
      let histTimer = 0;
      const queueSnap = () => { clearTimeout(histTimer); histTimer = setTimeout(snapState, 350); };
      apf.on("time", queueSnap); apf.on("pitch", queueSnap);
      snapState();

      /* readouts */
      const co = { pitch: $("#co-pitch"), time: $("#co-time"), drift: $("#co-drift"), ripple: $("#co-ripple") };
      const echoLed = $("#echo-led");
      let ledShown = 0;

      /* the portal */
      const cv = $("#portal"), c2 = cv.getContext("2d");
      const parts = Array.from({ length: 34 }, (_, i) => ({ a: (i * 2.39996) % (2 * Math.PI), r: 0.35 + ((i * 0.618) % 1) * 0.65, s: 0.4 + ((i * 0.37) % 1), z: (i * 0.13) % 1 }));
      let spin = 0, warpShown = 0, phaseShown = 0;
      const rgbv = (n, a) => `rgba(${getComputedStyle(document.documentElement).getPropertyValue(n).trim()},${a})`;
      function drawPortal(dt, ex, stale) {
        const s = K.sizeCanvas(cv, c2), w = s.w, h = s.h, cx = w / 2, cy = h / 2 - h * 0.04, R = Math.min(w, h) * 0.4;
        const c = c2; c.clearRect(0, 0, w, h);
        const light = look === "light";
        const bg = c.createRadialGradient(cx, cy, R * 0.1, cx, cy, Math.max(w, h) * 0.7);
        bg.addColorStop(0, light ? "#f2f0fa" : "#140b26"); bg.addColorStop(1, light ? "#cfcbe0" : "#040309");
        c.fillStyle = bg; c.fillRect(0, 0, w, h);
        const time = plainNow("time") / 100, pitch = stale ? plainNow("pitch") : (ex[1] ?? plainNow("pitch"));
        const playing = !stale;
        spin += dt * (playing ? 0.35 + 0.9 * (time - 0.5) : 0.08) * (time >= 1 ? 1 : 0.7);
        warpShown += (pitch / 12 - warpShown) * (1 - Math.exp(-dt / 0.12));
        const cA = "--accent-rgb", cB = "--accent-2-rgb", cH = "--accent-hot-rgb";
        // volumetric glow
        const glow = c.createRadialGradient(cx, cy, 0, cx, cy, R * 1.25);
        glow.addColorStop(0, rgbv(cH, light ? 0.35 : 0.5)); glow.addColorStop(0.35, rgbv(cA, 0.18)); glow.addColorStop(1, rgbv(cA, 0));
        c.fillStyle = glow; c.beginPath(); c.arc(cx, cy, R * 1.25, 0, 7); c.fill();
        // rings
        const rings = 9;
        for (let i = 0; i < rings; i++) {
          const f = (i + 1) / rings;
          const rr = R * (0.12 + 0.88 * Math.pow(f, 1.0 + warpShown * 0.6));
          const rot = spin * (1.4 - f) * (i % 2 ? -1 : 1);
          const ecc = 1 + 0.08 * warpShown * Math.sin(spin * 2 + i);
          c.save(); c.translate(cx, cy); c.rotate(rot); c.scale(ecc, 1 / ecc);
          c.lineWidth = Math.max(1, (1 - f) * 3 + 1);
          c.strokeStyle = i % 3 === 0 ? rgbv(cH, 0.75 - f * 0.4) : i % 3 === 1 ? rgbv(cA, 0.65 - f * 0.3) : rgbv(cB, 0.55 - f * 0.25);
          c.setLineDash([rr * (0.5 + f), rr * 0.18]);
          c.beginPath(); c.arc(0, 0, rr, 0, Math.PI * 2); c.stroke();
          c.restore();
        }
        c.setLineDash([]);
        // tear in the middle: the "other side"
        const core = c.createRadialGradient(cx, cy, 0, cx, cy, R * 0.16);
        core.addColorStop(0, light ? "#ffffff" : "#fff"); core.addColorStop(0.5, rgbv(cH, 0.9)); core.addColorStop(1, rgbv(cA, 0));
        c.fillStyle = core; c.beginPath(); c.arc(cx, cy, R * 0.16 * (1 + 0.15 * Math.sin(spin * 3)), 0, 7); c.fill();
        // particles fall in
        for (const p of parts) {
          p.z -= dt * 0.08 * p.s * (0.6 + time);
          if (p.z < 0) p.z += 1;
          const rr = R * (0.15 + 1.0 * p.z) * p.r * 1.3;
          const a = p.a + spin * (1.5 - p.z);
          const x = cx + Math.cos(a) * rr, y = cy + Math.sin(a) * rr * 0.92;
          c.fillStyle = rgbv(p.s > 0.9 ? cH : cA, 0.25 + 0.6 * (1 - p.z));
          c.beginPath(); c.arc(x, y, 0.9 + 1.6 * (1 - p.z), 0, 7); c.fill();
        }
        // the warp period arc
        const ph = stale ? 0 : ex[3] ?? 0;
        phaseShown = ph < phaseShown - 0.5 ? ph : phaseShown + (ph - phaseShown) * Math.min(1, dt * 20);
        const warping = Math.abs(time - 1) > 0.004;
        c.lineWidth = 3; c.lineCap = "round";
        c.strokeStyle = light ? "rgba(0,0,0,.12)" : "rgba(255,255,255,.08)";
        c.beginPath(); c.arc(cx, cy, R * 1.08, 0, Math.PI * 2); c.stroke();
        c.strokeStyle = warping ? rgbv(cH, 0.95) : rgbv(cA, 0.35);
        c.beginPath(); c.arc(cx, cy, R * 1.08, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * phaseShown); c.stroke();
        c.lineCap = "butt";
        // labels
        const R2 = K.rem();
        c.font = `600 ${(0.64 * R2).toFixed(1)}px "JetBrains Mono", monospace`; c.textAlign = "center"; c.textBaseline = "top";
        c.fillStyle = light ? "rgba(30,20,60,.65)" : "rgba(220,210,255,.6)";
        c.fillText(`${P("warp_mode").text.toUpperCase()} · ${P("warp_sync").text.toUpperCase()} · ${warping ? "WARPING" : "IN SYNC"}`, cx, cy + R * 1.08 + 0.5 * R2);
      }

      const spec = ctx.spectrum($("#spec"), { eqAxis: false, top: 0.5, dbTop: -6, dbBot: -78, hint: "Input and output spectrum." });
      const meters = ctx.levelMeters($("#meters"), { gr: false });
      ctx.onDraw((dt) => {
        const ex = ctx.extra || [], stale = ctx.stale();
        drawPortal(dt, ex, stale);
        spec.draw(dt);
        meters.draw(dt);
        const pst = stale || ex.length < 5 ? plainNow("pitch") : ex[1];
        co.pitch.textContent = K.sign((pst > 0 ? "+" : "") + pst.toFixed(2));
        co.time.textContent = Math.round(plainNow("time"));
        co.drift.textContent = K.sign((stale ? 0 : ex[2] || 0).toFixed(1));
        co.ripple.textContent = (stale || ex.length < 5 ? Math.pow(2, plainNow("pitch") / 12) * (ctx.idx("warp_mode") === 1 ? plainNow("time") / 100 : 1) : ex[4]).toFixed(3);
        const v = stale ? 0 : K.clamp01((K.lin2db(ex[0] || 0) + 54) / 48);
        ledShown = v > ledShown ? v : Math.max(v, ledShown - dt * 1.6);
        echoLed.style.setProperty("--lv", ledShown.toFixed(3));
      });
    },
  });
})();
