// NoDAW Chronos Dynamic EQ — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { DynBand, BandKind, EqBand, Shape, Smooth, dbToGain, undenorm, clamp } from '../shared/dsp.js';
import { register, idx } from '../shared/worklet-base.js';
const BANDS = 7;
class Engine {
  constructor(sr) { this.sr = sr; const sm = (v, ms) => new Smooth(v, sr, ms, ms);
    this.bands = Array.from({ length: BANDS }, () => new DynBand(sr)); this.aud = Array.from({ length: BANDS }, () => new EqBand(Math.floor(sr * 0.02))); this.wasOn = Array(BANDS).fill(true);
    this.inG = sm(1, 15); this.outG = sm(1, 15); this.mix = sm(1, 20); this.bypass = sm(0, 8); this.soloAmt = sm(0, 10); this.fresh = true; this.o = [0, 0]; }
  process(l, r, s) {
    const n = l.length, nyq = this.sr * 0.45, set = [];
    for (let i = 0; i < BANDS; i++) { const b = Object.assign({}, s.bands[i]); b.range_db = Math.min(b.range_db * clamp(s.dyn_amount, 0, 2), 24); b.freq = Math.min(clamp(b.freq, 10, 24000), nyq); set.push(b);
      if (b.on && !this.wasOn[i]) this.bands[i].reset(); this.wasOn[i] = b.on; this.bands[i].prepare(b);
      this.aud[i].set(b.kind === 0 ? Shape.BandPass : b.kind === 1 ? Shape.LowPass : Shape.HighPass, b.freq, Math.max(b.q, 0.5), 0, this.sr); }
    const anySolo = s.bands.some(b => b.solo && b.on), inT = dbToGain(s.in_db), outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0, soloT = anySolo ? 1 : 0;
    if (this.fresh) { this.fresh = false; this.inG.reset(inT); this.outG.reset(outT); this.mix.reset(s.mix); this.bypass.reset(bypT); this.soloAmt.reset(soloT); for (const a of this.aud) a.reset(); }
    const soloActive = anySolo || this.soloAmt.value > 0, o = this.o;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0, g = this.inG.tick(inT);
      let a = xl * g, b = xr * g, sl = 0, sr = 0;
      for (let k = 0; k < BANDS; k++) {
        if (!set[k].on) { this.aud[k].advance(); continue; }
        if (soloActive) { if (s.bands[k].solo) { this.aud[k].tick(a, b, o); sl += o[0]; sr += o[1]; } else this.aud[k].advance(); }
        this.bands[k].tick(a, b, set[k], o); a = o[0]; b = o[1];
      }
      if (soloActive) { const sa = this.soloAmt.tick(soloT); a += (sl - a) * sa; b += (sr - b) * sa; } else for (const au of this.aud) au.advance();
      const og = this.outG.tick(outT), mx = this.mix.tick(s.mix), ol = xl + (a * og - xl) * mx, or = xr + (b * og - xr) * mx, bb = this.bypass.tick(bypT);
      l[i] = undenorm(ol + (xl - ol) * bb); r[i] = undenorm(or + (xr - or) * bb);
    }
    return { dyn: this.bands.map((b, k) => (set[k].on ? b.gainDb : 0)) };
  }
}
function settings(p) {
  const bands = [];
  for (let i = 1; i <= BANDS; i++) bands.push({ on: p[`b${i}_on`] !== false, kind: idx(['Bell', 'Low Shelf', 'High Shelf'], p[`b${i}_type`]), freq: p[`b${i}_freq`], q: p[`b${i}_q`], gain_db: p[`b${i}_gain`],
    mode: idx(['Static', 'Compress', 'Expand'], p[`b${i}_mode`]), threshold_db: p[`b${i}_thresh`], ratio: p[`b${i}_ratio`], range_db: p[`b${i}_range`], attack_ms: p[`b${i}_attack`], release_ms: p[`b${i}_release`], solo: !!p[`b${i}_solo`] });
  return { bypass: !!p.bypass, in_db: p.in_gain, out_db: p.out_gain, mix: p.mix * 0.01, dyn_amount: p.dyn_amount * 0.01, bands };
}
register('nodaw-chronos-dynamic-eq', sr => new Engine(sr), settings);
