/* NoDAW Racks: plugin UI on the NoDAW kit (ui-kit/kit.js).
 *
 * Four modules in a rack, shown in signal order; drag a module's header to reorder (the order
 * is the "order" parameter, so it is automatable and saved). The analyzer shows the EQ curve
 * with a node per band: drag to move, scroll for Q, double-click to flatten. The skin is page
 * state saved with the session (controller "x").
 */
(() => {
  "use strict";
  const K = NoDAW, $ = K.$, $$ = K.$$;
  K.fitRoot();
  K.shell({ name: "NoDAW Racks", prod: "RACKS", version: "v0.1" });

  /* ── controls ─────────────────────────────────────────────────────────────────────── */
  const BAND_KEYS = [
    { n: 1, on: "eq1_on", f: "eq1_freq", g: "eq1_gain", q: "eq1_q", type: "eq1_type", types: ["Shelf", "Cut", "Bell"] },
    { n: 2, on: "eq2_on", f: "eq2_freq", g: "eq2_gain", q: "eq2_q" },
    { n: 3, on: "eq3_on", f: "eq3_freq", g: "eq3_gain", q: "eq3_q" },
    { n: 4, on: "eq4_on", f: "eq4_freq", g: "eq4_gain", q: "eq4_q" },
    { n: 5, on: "eq5_on", f: "eq5_freq", g: "eq5_gain", q: "eq5_q" },
    { n: 6, on: "eq6_on", f: "eq6_freq", g: "eq6_gain", q: "eq6_q" },
    { n: 7, on: "eq7_on", f: "eq7_freq", g: "eq7_gain", q: "eq7_q", type: "eq7_type", types: ["Shelf", "Cut", "Bell"] },
  ];
  const BAND_COLORS = ["rgb(244,63,94)", "rgb(249,115,22)", "rgb(234,179,8)", "rgb(34,197,94)", "rgb(6,182,212)", "rgb(59,130,246)", "rgb(168,85,247)"];

  K.knobs({
    "g-sat": [
      { k: "sat_drive", hint: "How hard the signal hits the curve. Auto Gain keeps the level steady so you hear colour, not volume." },
      { k: "sat_tone", bipolar: true, mini: true, hint: "Tilts the saturated sound darker or brighter around 1 kHz." },
      { k: "sat_mix", mini: true, hint: "Parallel blend of the saturated signal with the clean one (latency-aligned)." },
    ],
    "g-sp": [
      { k: "sp_width", tiny: true, hint: "Stereo width: 0% mono, 100% unchanged, 200% extra wide." },
      { k: "sp_haas", tiny: true, bipolar: true, hint: "Haas delay: delays one side by up to 30 ms (negative: left, positive: right) for width from a mono source." },
      { k: "sp_pan", tiny: true, bipolar: true, hint: "Balance between left and right." },
      { k: "sp_mid", tiny: true, bipolar: true, hint: "Mid (centre) level in the M/S matrix." },
      { k: "sp_side", tiny: true, bipolar: true, hint: "Side (stereo) level in the M/S matrix." },
      { k: "sp_mono", tiny: true, hint: "Mono Bass: everything below this frequency is folded to mono. Far left is off." },
    ],
    "g-lim": [
      { k: "lim_gain", hint: "Push: gain into the limiter. More push, more loudness and gain reduction." },
      { k: "lim_ceiling", mini: true, hint: "True-peak ceiling: the output never goes above it." },
      { k: "lim_release", mini: true, hint: "Release: how fast the limiter lets go. Adaptive slows it down when it works hard." },
    ],
    "g-master": [
      { k: "in_gain", tiny: true, bipolar: true, hint: "Input gain into the rack." },
      { k: "mix", tiny: true, hint: "Dry/wet of the whole rack. The dry path is latency-aligned." },
      { k: "out_gain", tiny: true, bipolar: true, hint: "Output gain after the rack." },
    ],
  });
  $("#sat-curve").append(K.choice("sat_curve"));
  $("#sat-auto").append(K.pill("sat_auto", "Auto gain"));
  $("#lim-adapt").append(K.pill("lim_adaptive", "Adaptive"));
  for (const slot of $$("[data-pill]")) slot.append(K.pill(slot.dataset.pill, "On", "pwr"));

  const tabs = $("#eq-tabs"), bandsBox = $("#eq-bands");
  BAND_KEYS.forEach((b, i) => {
    const t = K.h("button", "", String(b.n)); t.type = "button"; t.setAttribute("role", "tab"); t.style.setProperty("--bc", BAND_COLORS[i]); t.dataset.i = i;
    t.title = `Band ${b.n}`;
    tabs.append(t);
    const g = K.h("div", "band"); g.dataset.i = i; g.style.setProperty("--bc", BAND_COLORS[i]);
    g.append(
      K.knob({ k: b.f, mini: true, hint: `Band ${b.n} frequency. Drag its node on the analyzer too.` }),
      K.knob({ k: b.g, mini: true, bipolar: true, hint: `Band ${b.n} gain. Drag its node up or down on the analyzer.` }),
      K.knob({ k: b.q, mini: true, hint: `Band ${b.n} Q (bandwidth; slope for shelves). Scroll on its node to change it.` }),
    );
    const opts = K.h("div", "opts");
    opts.append(K.pill(b.on, "Band on"));
    if (b.type) opts.append(K.choice(b.type, b.types));
    g.append(opts);
    bandsBox.append(g);
  });

  // chain order permutations, the same order as the plugin's ORDERS table (lexicographic)
  const ORDERS = [];
  (function perm(prefix, rest) { if (!rest.length) { ORDERS.push(prefix); return; } rest.forEach((v, i) => perm(prefix.concat(v), rest.slice(0, i).concat(rest.slice(i + 1)))); })([], [0, 1, 2, 3]);

  K.start({
    init: async (ctx) => {
      const { P, plainNow, on, idx } = ctx;
      const mods = $$(".mod");

      /* selected EQ band */
      let sel = 3;
      function select(i) {
        sel = i;
        $$("#eq-tabs button").forEach((b, j) => { b.classList.toggle("on", j === i); b.setAttribute("aria-selected", j === i); });
        $$("#eq-bands .band").forEach((g, j) => g.classList.toggle("on", j === i));
        spec.selected = i;
      }
      tabs.addEventListener("click", (e) => { const b = e.target.closest("button"); if (b) select(+b.dataset.i); });

      /* EQ maths for the curve (the same filters as the DSP) */
      const shapeOf = (i) => {
        if (i === 0) return ["lowShelf", "highpass", "peak"][idx("eq1_type")] || "lowShelf";
        if (i === 6) return ["highShelf", "lowpass", "peak"][idx("eq7_type")] || "highShelf";
        return "peak";
      };
      const bandDb = (i, f, sr) => {
        const b = BAND_KEYS[i];
        return K.bq.shapeDb(shapeOf(i), plainNow(b.f), plainNow(b.q), plainNow(b.g), f, sr);
      };
      const bandLive = (i) => on("eq_on") && on(BAND_KEYS[i].on);
      const spec = ctx.spectrum($("#spec"), {
        eqRange: 18,
        nodes: () => BAND_KEYS.map((b, i) => {
          const cut = shapeOf(i) === "highpass" || shapeOf(i) === "lowpass";
          return {
            id: i, fKey: b.f, gKey: cut ? null : b.g, qKey: b.q, f: plainNow(b.f), db: cut ? 0 : plainNow(b.g),
            color: BAND_COLORS[i], on: bandLive(i), badge: String(b.n),
            label: () => `${b.n} · ${P(b.f).text}${cut ? "" : " · " + K.sign(P(b.g).text)} · Q ${P(b.q).text}`,
          };
        }),
        bandCurve: (n, f, sr) => bandDb(n.id, f, sr),
        curve: (f, sr) => { let t = 0; for (let i = 0; i < 7; i++) if (bandLive(i)) t += bandDb(i, f, sr); return t; },
        onSelect: (n) => select(n.id),
        hint: "Drag a node to move its band (Shift for fine). Scroll on a node for Q. Double-click to flatten.",
      });
      select(3);
      const tiltBtn = $("#tilt");
      tiltBtn.addEventListener("click", () => { const t = spec.tilt === 3 ? 0 : spec.tilt === 0 ? 4.5 : 3; spec.setTilt(t); tiltBtn.textContent = t ? `${t} dB/oct` : "Flat"; tiltBtn.classList.toggle("on", t !== 3); });
      for (const b of BAND_KEYS) apf.on(b.on, () => $$("#eq-tabs button").forEach((t, j) => t.classList.toggle("live", bandLive(j))));
      apf.on("eq_on", () => $$("#eq-tabs button").forEach((t, j) => t.classList.toggle("live", bandLive(j))));

      /* module on/off */
      const ONKEY = ["sat_on", "eq_on", "sp_on", "lim_on"];
      ONKEY.forEach((k, m) => apf.on(k, (v) => mods[m].classList.toggle("off", v < 0.5)));

      /* chain order: CSS order, drag to reorder */
      const orderNow = () => ORDERS[Math.min(23, idx("order"))];
      function layout() { orderNow().forEach((m, pos) => { mods[m].style.order = pos; }); $(".master").style.order = 9; }
      apf.on("order", layout);
      let drag = null;
      $$(".grip").forEach((g) => {
        const mod = g.closest(".mod"), m = +mod.dataset.mod;
        g.addEventListener("pointerdown", (e) => { g.setPointerCapture(e.pointerId); drag = { m, target: null, after: false }; mod.classList.add("dragging"); e.preventDefault(); });
        g.addEventListener("pointermove", (e) => {
          if (!drag) return;
          mods.forEach((x) => x.classList.remove("drop-before", "drop-after"));
          const over = mods.find((x) => { const r = x.getBoundingClientRect(); return e.clientX >= r.left && e.clientX <= r.right; });
          if (!over || +over.dataset.mod === drag.m) { drag.target = null; return; }
          const r = over.getBoundingClientRect(); drag.target = +over.dataset.mod; drag.after = e.clientX > r.left + r.width / 2;
          over.classList.add(drag.after ? "drop-after" : "drop-before");
        });
        const end = () => {
          if (!drag) return;
          mods.forEach((x) => x.classList.remove("drop-before", "drop-after", "dragging"));
          if (drag.target !== null) {
            const cur = orderNow().filter((x) => x !== drag.m);
            const at = cur.indexOf(drag.target) + (drag.after ? 1 : 0);
            cur.splice(at, 0, drag.m);
            const i = ORDERS.findIndex((o) => o.join() === cur.join());
            const p = P("order"), from = p.value, to = i / p.steps;
            if (i >= 0 && Math.abs(from - to) > 1e-9) { apf.change("order", to); ctx.recordParam("order", from, to); }
          }
          drag = null;
        };
        g.addEventListener("pointerup", end); g.addEventListener("pointercancel", end);
        g.addEventListener("pointerenter", () => ctx.hint("Drag to move this module in the chain."));
        g.addEventListener("pointerleave", () => ctx.hint(""));
      });

      /* skins (session state through the controller) */
      let skin = "cobalt";
      function applySkin(s) {
        skin = ["cobalt", "obsidian", "silver", "plastic"].includes(s) ? s : "cobalt";
        const app = $("#app");
        app.className = app.className.replace(/\bskin-\w+/g, "").trim() + ` skin-${skin}`;
        $$("#skins button").forEach((b) => b.classList.toggle("on", b.dataset.skin === skin));
        K.refreshDefs();
      }
      ctx.ext = {
        get: () => ({ skin }),
        set: (o) => { applySkin(o && o.skin); apf.plugin.post("skin", JSON.stringify(skin)).catch((e) => apf.log("skin:", String(e))); },
        refresh: async () => { try { applySkin(await apf.plugin.get("skin")); } catch (e) { applySkin("cobalt"); } },
      };
      $("#skins").addEventListener("click", (e) => {
        const b = e.target.closest("button"); if (!b) return;
        const from = ctx.ext.get(); ctx.ext.set({ skin: b.dataset.skin }); ctx.recordExt(from, ctx.ext.get());
      });

      /* meters, module VUs, correlation, GR */
      const meters = ctx.levelMeters($("#meters"), { grLabel: "GR" });
      const vus = mods.map((m) => $(".vu b", m));
      const vuShown = [0, 0, 0, 0];
      let corrShown = 1;
      const corrBar = $("#corr b"), grRead = $("#gr-read");
      ctx.onDraw((dt) => {
        spec.draw(dt);
        meters.draw(dt);
        const ex = ctx.extra || [], stale = ctx.stale();
        for (let m = 0; m < 4; m++) {
          const db = stale ? -60 : K.lin2db(ex[m] || 0);
          const v = K.clamp01((db + 48) / 48);
          vuShown[m] = v > vuShown[m] ? v : Math.max(v, vuShown[m] - dt * 1.4);
          vus[m].style.width = `${(vuShown[m] * 100).toFixed(1)}%`;
        }
        const c = stale || ex.length < 5 ? 1 : ex[4];
        corrShown += (c - corrShown) * (1 - Math.exp(-dt / 0.15));
        corrBar.style.left = `${((corrShown + 1) / 2) * 100}%`;
        const gr = K.lin2db(ctx.meters.gr);
        grRead.textContent = `GR ${gr < -0.05 && !stale ? K.sign(gr.toFixed(1)) : "0.0"} dB`;
        grRead.classList.toggle("on", gr < -0.5 && !stale);
      });
      layout();
    },
  });
})();
