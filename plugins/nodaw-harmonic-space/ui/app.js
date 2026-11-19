/* NoDAW Harmonic Space: plugin UI on the NoDAW kit (ui-kit/kit.js).
 *
 * The Spectral Space Designer's faceplate, rebuilt: the Space Recipes strip (the factory
 * presets, by family), the analyzer with the whole shape (7 EQ bands, sub, HPF, LPF) and a node
 * per band, the effects row (amp, filters, sub, delay, reverb, flanger) and the 7-band strip
 * with solo and mute. The faceplate (dark or brushed aluminium) is session state ("x").
 */
(() => {
  "use strict";
  const K = NoDAW, $ = K.$, $$ = K.$$;
  K.fitRoot(960, 620);
  K.shell({ name: "NoDAW Harmonic Space", prod: "HARMONIC SPACE", version: "v0.1" });

  const COLORS = ["rgb(244,63,94)", "rgb(249,115,22)", "rgb(234,179,8)", "rgb(34,197,94)", "rgb(6,182,212)", "rgb(59,130,246)", "rgb(168,85,247)"];
  const BANDS = [
    { n: 1, f: "b1_freq", g: "b1_gain", q: "b1_q", solo: "b1_solo", mute: "b1_mute" },
    { n: 2, f: "b2_freq", g: "b2_gain", q: "b2_q", solo: "b2_solo", mute: "b2_mute" },
    { n: 3, f: "b3_freq", g: "b3_gain", q: "b3_q", solo: "b3_solo", mute: "b3_mute" },
    { n: 4, f: "b4_freq", g: "b4_gain", q: "b4_q", solo: "b4_solo", mute: "b4_mute" },
    { n: 5, f: "b5_freq", g: "b5_gain", q: "b5_q", solo: "b5_solo", mute: "b5_mute" },
    { n: 6, f: "b6_freq", g: "b6_gain", q: "b6_q", solo: "b6_solo", mute: "b6_mute" },
    { n: 7, f: "b7_freq", g: "b7_gain", q: "b7_q", solo: "b7_solo", mute: "b7_mute" },
  ];
  const SHAPE = (i) => (i === 0 ? "lowShelf" : i === 6 ? "highShelf" : "peak");
  const MUTE_DB = -36;

  K.knobs({
    "g-amp": [{ k: "amp", bipolar: true, mini: true, hint: "Amp: clean gain into the EQ and effects (the prototype's AMP)." }],
    "g-filt": [
      { k: "hpf", mini: true, hint: "High-pass filter (12 dB/oct). Far left is off." },
      { k: "lpf", mini: true, hint: "Low-pass filter (12 dB/oct). Far right is off." },
    ],
    "g-sub": [
      { k: "sub", mini: true, hint: "Sub: a low shelf boost for weight." },
      { k: "sub_freq", mini: true, hint: "Where the sub shelf sits." },
    ],
    "g-dly-main": [{ k: "dly_mix", mini: true, label: "Delay", hint: "Delay return level. The echoes are of the EQ'd, filtered signal." }],
    "g-dly": [
      { k: "dly_time", tiny: true, label: "Time", hint: "Time between echoes. Glides like tape when it moves." },
      { k: "dly_fb", tiny: true, label: "Feedback", hint: "How many echoes: feedback of the delay line." },
      { k: "dly_tone", tiny: true, label: "Tone", hint: "Darkens each repeat (low-pass in the feedback path)." },
    ],
    "g-rev-main": [{ k: "rev_mix", mini: true, label: "Reverb", hint: "Reverb return level." }],
    "g-rev": [
      { k: "rev_decay", tiny: true, hint: "Decay (RT60): how long the room rings." },
      { k: "rev_size", tiny: true, hint: "Room size: the spacing of the early reflections." },
      { k: "rev_pre", tiny: true, label: "Pre-delay", hint: "Pre-delay before the room starts." },
      { k: "rev_damp", tiny: true, hint: "Damping: lower is a darker, softer room." },
    ],
    "g-fl-main": [{ k: "fl_mix", mini: true, label: "Flanger", hint: "Flanger return level." }],
    "g-fl": [
      { k: "fl_rate", tiny: true, label: "Rate", hint: "Sweep rate of the flanger." },
      { k: "fl_depth", tiny: true, label: "Depth", hint: "Sweep depth." },
      { k: "fl_fb", tiny: true, bipolar: true, label: "Feedback", hint: "Feedback: positive is a metallic resonance, negative a hollow one." },
    ],
    "g-master": [
      { k: "out_gain", bipolar: true, label: "Output", hint: "Output level (the prototype's main knob)." },
      { k: "width", tiny: true, hint: "Stereo width of the result: 0% mono, 200% extra wide." },
      { k: "mix", tiny: true, hint: "Dry/wet of the whole plugin." },
    ],
  });
  { const c = K.h("div", "pp-cell"); c.append(K.pill("dly_pp", "Ping-pong")); $("#g-dly").append(c); }

  // the band strip
  const eqBox = $("#eq");
  BANDS.forEach((b, i) => {
    const col = K.h("div", "band"); col.dataset.i = i; col.style.setProperty("--bc", COLORS[i]);
    const head = K.h("div", "b-head");
    head.innerHTML = `<span class="b-n">${b.n}</span><span class="b-f" data-param-text="${b.f}"></span>`;
    const sm = K.h("span", "b-sm");
    sm.append(K.pill(b.solo, "S", "solo-p"), K.pill(b.mute, "M", "mute-p"));
    head.append(sm);
    const knobs = K.h("div", "b-knobs");
    knobs.append(
      K.knob({ k: b.g, mini: true, bipolar: true, label: "Gain", hint: `Band ${b.n} gain (±12 dB). Drag its node on the analyzer too.` }),
      K.knob({ k: b.q, tiny: true, label: "Q", hint: `Band ${b.n} Q (bandwidth; slope for the shelves). Scroll on its node too.` }),
      K.knob({ k: b.f, tiny: true, label: "Freq", hint: `Band ${b.n} frequency.` }),
    );
    col.append(head, knobs);
    eqBox.append(col);
  });

  K.start({
    init: async (ctx) => {
      const { P, plainNow, on } = ctx;

      /* the whole static shape, in the DSP's filters */
      const anySolo = () => BANDS.some((b) => on(b.solo));
      const bandDb = (i, f, sr) => {
        const b = BANDS[i];
        const g = on(b.mute) ? MUTE_DB : plainNow(b.g);
        return K.bq.shapeDb(SHAPE(i), plainNow(b.f), plainNow(b.q), g, f, sr);
      };
      const soloDb = (f, sr) => {
        // the audition: the soloed bands' regions summed (as power, close enough for a picture)
        let p = 0;
        BANDS.forEach((b, i) => {
          if (!on(b.solo)) return;
          const shape = i === 0 ? "lowpass" : i === 6 ? "highpass" : "bandpass";
          p += Math.pow(10, K.bq.shapeDb(shape, plainNow(b.f), Math.max(0.5, plainNow(b.q)), 0, f, sr) / 10);
        });
        return 10 * Math.log10(p + 1e-9);
      };
      const toneDb = (f, sr) => {
        let t = K.bq.shapeDb("lowShelf", plainNow("sub_freq"), 0.8, plainNow("sub"), f, sr);
        const hp = plainNow("hpf"), lp = plainNow("lpf");
        if (hp > 20.5) t += K.bq.shapeDb("highpass", hp, 0.707, 0, f, sr);
        if (lp < 21900 && lp < sr * 0.45) t += K.bq.shapeDb("lowpass", lp, 0.707, 0, f, sr);
        return t;
      };
      let sel = 3;
      function select(i) {
        sel = i;
        $$("#eq .band").forEach((c, j) => c.classList.toggle("picked", j === i));
        spec.selected = i;
      }
      const spec = ctx.spectrum($("#spec"), {
        eqRange: 18,
        nodes: () => [
          ...BANDS.map((b, i) => ({
            id: i, fKey: b.f, gKey: b.g, qKey: b.q, f: plainNow(b.f), db: on(b.mute) ? -12 : plainNow(b.g), color: COLORS[i],
            on: !on(b.mute) && (!anySolo() || on(b.solo)), badge: String(b.n),
            label: () => `${b.n} · ${P(b.f).text} · ${K.sign(P(b.g).text)} · Q ${P(b.q).text}${on(b.solo) ? " · SOLO" : on(b.mute) ? " · MUTE" : ""}`,
          })),
          { id: "hpf", fKey: "hpf", f: plainNow("hpf"), db: -15, color: "rgb(200,200,210)", on: plainNow("hpf") > 20.5, badge: "H", label: () => `High-pass · ${P("hpf").text}` },
          { id: "lpf", fKey: "lpf", f: plainNow("lpf"), db: -15, color: "rgb(200,200,210)", on: plainNow("lpf") < 21900, badge: "L", label: () => `Low-pass · ${P("lpf").text}` },
        ],
        bandCurve: (n, f, sr) => (typeof n.id === "number" ? bandDb(n.id, f, sr) : n.id === "hpf" ? (plainNow("hpf") > 20.5 ? K.bq.shapeDb("highpass", plainNow("hpf"), 0.707, 0, f, sr) : 0) : (plainNow("lpf") < 21900 ? K.bq.shapeDb("lowpass", plainNow("lpf"), 0.707, 0, f, sr) : 0)),
        curve: (f, sr) => {
          let t = anySolo() ? soloDb(f, sr) : 0;
          for (let i = 0; i < 7; i++) t += bandDb(i, f, sr);
          return t + plainNow("amp") + toneDb(f, sr);
        },
        onSelect: (n) => { if (typeof n.id === "number") select(n.id); },
        hint: "Drag a node to move its band (Shift for fine). Scroll on a node for Q. Double-click to flatten. H and L are the filters.",
      });
      select(3);
      const tiltBtn = $("#tilt");
      tiltBtn.addEventListener("click", () => { const t = spec.tilt === 3 ? 0 : spec.tilt === 0 ? 4.5 : 3; spec.setTilt(t); tiltBtn.textContent = t ? `${t} dB/oct` : "Flat"; tiltBtn.classList.toggle("on", t !== 3); });
      $$("#eq .band").forEach((c) => c.addEventListener("pointerdown", () => select(+c.dataset.i)));
      const soloNote = $("#solo-note");
      const soloState = () => {
        const s = anySolo();
        soloNote.hidden = !s;
        $$("#eq .band").forEach((c, j) => { c.classList.toggle("dim", s && !on(BANDS[j].solo)); c.classList.toggle("muted", on(BANDS[j].mute)); });
      };
      for (const b of BANDS) { apf.on(b.solo, soloState); apf.on(b.mute, soloState); }

      /* effect cards light up when their return is up */
      const cards = { delay: "dly_mix", reverb: "rev_mix", flanger: "fl_mix", sub: "sub" };
      for (const [fx, k] of Object.entries(cards)) apf.on(k, (v) => $(`.fxc[data-fx="${fx}"]`).classList.toggle("idle", plainNow(k) <= 0.0001));
      const filt = () => $(`.fxc[data-fx="filters"]`).classList.toggle("idle", plainNow("hpf") <= 20.5 && plainNow("lpf") >= 21900);
      apf.on("hpf", filt); apf.on("lpf", filt);

      /* recipes: the factory presets by family */
      const fams = $("#rc-fams"), list = $("#rc-list");
      let recipes = [], fam = null, current = null;
      function renderList() {
        list.textContent = "";
        for (const r of recipes.filter((x) => x.type === fam)) {
          const b = K.h("button", "rc"); b.type = "button"; b.textContent = r.name; b.dataset.id = r.id; b.title = r.description || "";
          b.classList.toggle("on", current === r.name);
          b.addEventListener("click", async () => { try { await apf.presets.load(r.id); } catch (e) { apf.log("recipe:", String(e)); } });
          b.addEventListener("pointerenter", () => ctx.hint(r.description || r.name));
          b.addEventListener("pointerleave", () => ctx.hint(""));
          list.append(b);
        }
        $$("button", fams).forEach((t) => t.classList.toggle("on", t.dataset.fam === fam));
      }
      try {
        const r = await apf.presets.list();
        recipes = (r.presets || []).filter((p) => p.factory && p.type && p.type !== "Utility");
        const order = [];
        for (const p of recipes) if (!order.includes(p.type)) order.push(p.type);
        for (const f of order) { const t = K.h("button", "", f); t.type = "button"; t.dataset.fam = f; t.setAttribute("role", "tab"); t.addEventListener("click", () => { fam = f; renderList(); }); fams.append(t); }
        fam = order[0] || null;
      } catch (e) { apf.log("recipes:", String(e)); }
      apf.presets.on((p) => {
        current = p && p.name ? p.name : null;
        const hit = recipes.find((x) => x.name === current);
        if (hit) fam = hit.type;
        renderList();
      });
      renderList();

      /* faceplate (session state through the controller) */
      let look = "dark";
      function applyLook(s) {
        look = s === "light" ? "light" : "dark";
        const app = $("#app");
        app.className = app.className.replace(/\blook-\w+/g, "").trim() + ` look-${look}`;
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

      /* meters, effect LEDs, correlation */
      const meters = ctx.levelMeters($("#meters"), { gr: false });
      const leds = $$(".led");
      const ledShown = [0, 0, 0];
      let corrShown = 1;
      const corrBar = $("#corr b");
      ctx.onDraw((dt) => {
        spec.draw(dt);
        meters.draw(dt);
        const ex = ctx.extra || [], stale = ctx.stale();
        for (let k = 0; k < 3; k++) {
          const v = stale ? 0 : K.clamp01((K.lin2db(ex[k] || 0) + 54) / 48);
          ledShown[k] = v > ledShown[k] ? v : Math.max(v, ledShown[k] - dt * 1.6);
          leds[k].style.setProperty("--lv", ledShown[k].toFixed(3));
        }
        const c = stale || ex.length < 4 ? 1 : ex[3];
        corrShown += (c - corrShown) * (1 - Math.exp(-dt / 0.15));
        corrBar.style.left = `${((corrShown + 1) / 2) * 100}%`;
      });
      soloState();
    },
  });
})();
