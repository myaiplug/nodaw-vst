/* NoDAW Chronos Dynamic EQ: plugin UI on the NoDAW kit (ui-kit/kit.js).
 *
 * The analyzer shows the static curve (dashed) and the live curve with each band's dynamic
 * gain (the ring on its node, read from the plugin every frame). Below: a chip per band (a
 * chronograph ring for its gain change), the selected band's EQ and dynamics, and the master.
 * The colourway is page state saved with the session (controller "x").
 */
(() => {
  "use strict";
  const K = NoDAW, $ = K.$, $$ = K.$$;
  K.fitRoot(1000, 620);
  K.shell({ name: "NoDAW Chronos Dynamic EQ", prod: "CHRONOS DEQ", version: "v0.1" });

  const BANDS = [
    { n: 1, on: "b1_on", type: "b1_type", f: "b1_freq", g: "b1_gain", q: "b1_q", mode: "b1_mode", th: "b1_thresh", ra: "b1_ratio", rg: "b1_range", at: "b1_attack", re: "b1_release", solo: "b1_solo" },
    { n: 2, on: "b2_on", type: "b2_type", f: "b2_freq", g: "b2_gain", q: "b2_q", mode: "b2_mode", th: "b2_thresh", ra: "b2_ratio", rg: "b2_range", at: "b2_attack", re: "b2_release", solo: "b2_solo" },
    { n: 3, on: "b3_on", type: "b3_type", f: "b3_freq", g: "b3_gain", q: "b3_q", mode: "b3_mode", th: "b3_thresh", ra: "b3_ratio", rg: "b3_range", at: "b3_attack", re: "b3_release", solo: "b3_solo" },
    { n: 4, on: "b4_on", type: "b4_type", f: "b4_freq", g: "b4_gain", q: "b4_q", mode: "b4_mode", th: "b4_thresh", ra: "b4_ratio", rg: "b4_range", at: "b4_attack", re: "b4_release", solo: "b4_solo" },
    { n: 5, on: "b5_on", type: "b5_type", f: "b5_freq", g: "b5_gain", q: "b5_q", mode: "b5_mode", th: "b5_thresh", ra: "b5_ratio", rg: "b5_range", at: "b5_attack", re: "b5_release", solo: "b5_solo" },
    { n: 6, on: "b6_on", type: "b6_type", f: "b6_freq", g: "b6_gain", q: "b6_q", mode: "b6_mode", th: "b6_thresh", ra: "b6_ratio", rg: "b6_range", at: "b6_attack", re: "b6_release", solo: "b6_solo" },
    { n: 7, on: "b7_on", type: "b7_type", f: "b7_freq", g: "b7_gain", q: "b7_q", mode: "b7_mode", th: "b7_thresh", ra: "b7_ratio", rg: "b7_range", at: "b7_attack", re: "b7_release", solo: "b7_solo" },
  ];
  const COLORS = ["rgb(244,63,94)", "rgb(249,115,22)", "rgb(234,179,8)", "rgb(34,197,94)", "rgb(6,182,212)", "rgb(96,125,255)", "rgb(192,132,252)"];

  K.knobs({
    "g-master": [
      { k: "in_gain", tiny: true, bipolar: true, hint: "Input gain into the bands (moves the signal against every threshold)." },
      { k: "out_gain", tiny: true, bipolar: true, hint: "Output gain." },
      { k: "dyn_amount", tiny: true, hint: "Scales every band's dynamic range: 0% is a static EQ, 200% doubles the movement (at most 24 dB)." },
      { k: "mix", tiny: true, hint: "Dry/wet of the whole EQ." },
    ],
  });

  const chips = $("#chips"), panels = $("#panels");
  BANDS.forEach((b, i) => {
    const c = K.h("button", "chip"); c.type = "button"; c.dataset.i = i; c.style.setProperty("--bc", COLORS[i]); c.setAttribute("role", "tab");
    c.innerHTML = `<i class="ring"><b></b><span>${b.n}</span></i><span class="c-txt"><span class="c-f" data-param-text="${b.f}"></span><span class="c-g" data-param-text="${b.g}"></span></span><span class="c-m" data-param-text="${b.mode}"></span>`;
    chips.append(c);
    const p = K.h("div", "bp panel"); p.dataset.i = i; p.style.setProperty("--bc", COLORS[i]);
    const head = K.h("h3", "", `Band ${b.n} <i class="dyn-read" data-dyn="${i}">0.0 dB</i>`);
    const tools = K.h("span", "bp-tools");
    tools.append(K.pill(b.on, "On", "pwr"), K.pill(b.solo, "Solo", "solo-p"));
    head.append(tools);
    const eq = K.h("div", "bp-eq");
    const t = K.h("div", "bp-type"); t.append(K.choice(b.type, ["Bell", "Low", "High"]));
    eq.append(t, K.knob({ k: b.f, mini: true, label: "Freq", hint: `Band ${b.n} frequency. Drag its node on the analyzer too.` }),
      K.knob({ k: b.g, mini: true, bipolar: true, label: "Gain", hint: `Band ${b.n} static gain. The dynamics move from here.` }),
      K.knob({ k: b.q, mini: true, label: "Q", hint: `Band ${b.n} bandwidth (bells). Scroll on its node too.` }));
    const dy = K.h("div", "bp-dyn");
    const m = K.h("div", "bp-type"); m.append(K.choice(b.mode, ["Static", "Comp", "Expand"]));
    dy.append(m,
      K.knob({ k: b.th, mini: true, label: "Thresh", hint: `Band ${b.n} threshold: the band's own level (inside its filter) where the dynamics start.` }),
      K.knob({ k: b.ra, mini: true, label: "Ratio", hint: `Band ${b.n} ratio: how strongly the gain follows the level over the threshold.` }),
      K.knob({ k: b.rg, mini: true, label: "Range", hint: `Band ${b.n} range: the most the dynamics may cut (Compress) or lift (Expand).` }),
      K.knob({ k: b.at, mini: true, label: "Attack", hint: `Band ${b.n} attack time.` }),
      K.knob({ k: b.re, mini: true, label: "Release", hint: `Band ${b.n} release time.` }));
    p.append(head, K.h("div", "bp-body"));
    p.lastChild.append(eq, K.h("span", "bp-sep"), dy);
    panels.append(p);
  });

  K.start({
    init: async (ctx) => {
      const { P, plainNow, on, idx } = ctx;
      const dyn = new Float32Array(7);
      const live = (i) => on(BANDS[i].on);
      const anySolo = () => BANDS.some((b, i) => live(i) && on(b.solo));

      /* |1 + (G - 1) H| for the band's selection filter (nodaw-dsp DynBand) */
      const sel = (i, sr) => {
        const b = BANDS[i], kind = idx(b.type), f = plainNow(b.f);
        return kind === 1 ? K.bq.lowpass(f, 0.6, sr) : kind === 2 ? K.bq.highpass(f, 0.6, sr) : K.bq.bandpass(f, plainNow(b.q), sr);
      };
      const bandDb = (i, f, sr, withDyn) => {
        if (!live(i)) return 0;
        const g = plainNow(BANDS[i].g) + (withDyn ? dyn[i] : 0);
        if (Math.abs(g) < 1e-3) return 0;
        const c = sel(i, sr), w = (2 * Math.PI * f) / sr, c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
        const nr = c[0] + c[1] * c1 + c[2] * c2, ni = -(c[1] * s1 + c[2] * s2), dr = 1 + c[3] * c1 + c[4] * c2, di = -(c[3] * s1 + c[4] * s2);
        const den = dr * dr + di * di, hr = (nr * dr + ni * di) / den, hi = (ni * dr - nr * di) / den, k = Math.pow(10, g / 20) - 1;
        return 10 * Math.log10(Math.max(1e-12, (1 + k * hr) ** 2 + (k * hi) ** 2));
      };

      let selected = 3;
      function select(i) {
        selected = i;
        $$(".chip").forEach((c, j) => { c.classList.toggle("on", j === i); c.setAttribute("aria-selected", j === i); });
        $$(".bp").forEach((p, j) => p.classList.toggle("on", j === i));
        spec.selected = i;
      }
      chips.addEventListener("click", (e) => { const c = e.target.closest(".chip"); if (c) select(+c.dataset.i); });

      const staticCurve = (f, sr) => { let t = 0; for (let i = 0; i < 7; i++) t += bandDb(i, f, sr, false); return t; };
      const spec = ctx.spectrum($("#spec"), {
        eqRange: 24,
        nodes: () => BANDS.map((b, i) => ({
          id: i, fKey: b.f, gKey: b.g, qKey: b.q, f: plainNow(b.f), db: plainNow(b.g), color: COLORS[i],
          on: live(i) && (!anySolo() || on(b.solo)), badge: String(b.n), ring: idx(b.mode) ? dyn[i] : 0,
          label: () => `${b.n} · ${P(b.f).text} · ${K.sign(P(b.g).text)} · ${P(b.mode).text}${idx(b.mode) ? " " + K.sign(dyn[i].toFixed(1)) + " dB" : ""}`,
        })),
        bandCurve: (n, f, sr) => bandDb(n.id, f, sr, true),
        curve: (f, sr) => { let t = 0; for (let i = 0; i < 7; i++) t += bandDb(i, f, sr, true); return t; },
        over: (c, S) => {
          const L = S.L, sr = ctx.meters.sr || 48000;
          c.save(); c.setLineDash([4, 4]); c.strokeStyle = "rgba(255,255,255,.38)"; c.lineWidth = 1.1; c.beginPath();
          for (let i = 0; i <= 200; i++) { const f = 20 * Math.pow(1000, i / 200), y = S.ey(K.clamp(staticCurve(f, sr), -24, 24)); i ? c.lineTo(S.fx(f), y) : c.moveTo(S.fx(f), y); }
          c.stroke(); c.restore();
          // thresholds of the selected band as a marker on its node column
          const b = BANDS[selected];
          if (live(selected) && idx(b.mode)) {
            const x = S.fx(plainNow(b.f)), th = plainNow(b.th), y = S.dy(th);
            c.save(); c.strokeStyle = COLORS[selected]; c.globalAlpha = 0.75; c.lineWidth = 1.4; c.setLineDash([2, 3]);
            c.beginPath(); c.moveTo(x - 2.2 * K.rem(), y); c.lineTo(x + 2.2 * K.rem(), y); c.stroke();
            c.setLineDash([]); c.fillStyle = COLORS[selected]; c.font = `600 ${(0.56 * K.rem()).toFixed(1)}px "JetBrains Mono", monospace`; c.textAlign = "left";
            c.fillText(`THR ${K.sign(th.toFixed(0))}`, x + 2.4 * K.rem(), y + 0.2 * K.rem()); c.restore();
          }
        },
        onSelect: (n) => select(n.id),
        hint: "Drag a node to move its band (Shift for fine). Scroll on a node for Q. Double-click to flatten. Rings show the live dynamic gain.",
      });
      select(3);
      const tiltBtn = $("#tilt");
      tiltBtn.addEventListener("click", () => { const t = spec.tilt === 3 ? 0 : spec.tilt === 0 ? 4.5 : 3; spec.setTilt(t); tiltBtn.textContent = t ? `${t} dB/oct` : "Flat"; tiltBtn.classList.toggle("on", t !== 3); });

      const soloNote = $("#solo-note");
      const state = () => {
        const s = anySolo(); soloNote.hidden = !s;
        $$(".chip").forEach((c, j) => { c.classList.toggle("off", !live(j)); c.classList.toggle("dim", s && !on(BANDS[j].solo)); c.classList.toggle("dynamic", idx(BANDS[j].mode) > 0); });
        $$(".bp").forEach((p, j) => p.classList.toggle("static", idx(BANDS[j].mode) === 0));
      };
      for (const b of BANDS) for (const k of [b.on, b.solo, b.mode]) apf.on(k, state);

      /* colourway (session state through the controller) */
      let look = "violet";
      function applyLook(s) {
        look = s === "amber" ? "amber" : "violet";
        // on the root element, so the derived accent tokens and the canvases pick it up
        document.documentElement.classList.remove("look-violet", "look-amber");
        document.documentElement.classList.add(`look-${look}`);
        $$("#looks button").forEach((b) => b.classList.toggle("on", b.dataset.look === look));
        K.refreshDefs();
      }
      ctx.ext = {
        get: () => ({ skin: look }),
        set: (o) => { applyLook(o && o.skin); apf.plugin.post("skin", JSON.stringify(look)).catch((e) => apf.log("skin:", String(e))); },
        refresh: async () => { try { applyLook(await apf.plugin.get("skin")); } catch (e) { applyLook("violet"); } },
      };
      $("#looks").addEventListener("click", (e) => {
        const b = e.target.closest("button"); if (!b) return;
        const from = ctx.ext.get(); ctx.ext.set({ skin: b.dataset.look }); ctx.recordExt(from, ctx.ext.get());
      });

      /* meters, band rings */
      const meters = ctx.levelMeters($("#meters"), { grLabel: "DYN", grRange: 12 });
      const rings = $$(".chip .ring"), reads = $$(".dyn-read");
      ctx.onDraw((dt) => {
        const ex = ctx.extra || [], stale = ctx.stale();
        for (let i = 0; i < 7; i++) {
          const t = stale || ex.length < 7 || !live(i) ? 0 : ex[i];
          dyn[i] += (t - dyn[i]) * (1 - Math.exp(-dt / 0.06));
          const v = K.clamp(dyn[i] / 12, -1, 1);
          rings[i].style.setProperty("--d", v.toFixed(3));
          rings[i].classList.toggle("cut", v < -0.01); rings[i].classList.toggle("lift", v > 0.01);
          reads[i].textContent = `${K.sign(dyn[i].toFixed(1))} dB`;
          reads[i].classList.toggle("on", Math.abs(dyn[i]) > 0.05);
        }
        spec.draw(dt);
        meters.draw(dt);
      });
      state();
    },
  });
})();
