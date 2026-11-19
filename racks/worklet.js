// NoDAW Racks — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { DelayLine, EqBand, Shape, Limiter, Saturator, SatCurve, Correlation, balance, Smooth, dbToGain, undenorm, clamp } from '../shared/dsp.js';
import { register, idx } from '../shared/worklet-base.js';

const ORDER_NAMES = ['SAT > EQ > SPACE > LIM','SAT > EQ > LIM > SPACE','SAT > SPACE > EQ > LIM','SAT > SPACE > LIM > EQ','SAT > LIM > EQ > SPACE','SAT > LIM > SPACE > EQ',
 'EQ > SAT > SPACE > LIM','EQ > SAT > LIM > SPACE','EQ > SPACE > SAT > LIM','EQ > SPACE > LIM > SAT','EQ > LIM > SAT > SPACE','EQ > LIM > SPACE > SAT',
 'SPACE > SAT > EQ > LIM','SPACE > SAT > LIM > EQ','SPACE > EQ > SAT > LIM','SPACE > EQ > LIM > SAT','SPACE > LIM > SAT > EQ','SPACE > LIM > EQ > SAT',
 'LIM > SAT > EQ > SPACE','LIM > SAT > SPACE > EQ','LIM > EQ > SAT > SPACE','LIM > EQ > SPACE > SAT','LIM > SPACE > SAT > EQ','LIM > SPACE > EQ > SAT'];
const MOD = { SAT: 0, EQ: 1, SPACE: 2, LIM: 3 };
const ORDERS = ORDER_NAMES.map(n => n.split(' > ').map(k => MOD[k]));
const BANDS = 7, MAX_HAAS_MS = 30, LOOKAHEAD_MS = 1.5;

class Engine {
  constructor(sr) {
    this.sr = sr; this.limiter = new Limiter(sr, LOOKAHEAD_MS, 120); this.sat = new Saturator(); this.lat = this.sat.latency() + this.limiter.latency();
    const ramp = Math.floor(sr * 0.02), hl = Math.floor(MAX_HAAS_MS * 0.001 * sr) + 8;
    this.tone = [new EqBand(ramp), new EqBand(ramp)]; this.eq = Array.from({ length: BANDS }, () => new EqBand(ramp)); this.mono = new EqBand(ramp);
    this.haas = [new DelayLine(hl), new DelayLine(hl)]; this.dry = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)]; this.byp = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)];
    this.corr = new Correlation(sr); const sm = (v, ms) => new Smooth(v, sr, ms, ms);
    this.inG = sm(1, 15); this.outG = sm(1, 15); this.mix = sm(1, 20); this.bypass = sm(0, 8); this.drive = sm(1, 20); this.satMix = sm(1, 20); this.spAmt = sm(1, 20);
    this.width = sm(1, 20); this.mid = sm(1, 20); this.side = sm(1, 20); this.pan = sm(0, 20); this.haasD = sm(0, 60); this.limG = sm(1, 20); this.relFor = -1; this.fresh = true; this.o = [0, 0]; this.inited = false;
  }
  updateFilters(s) {
    const sr = this.sr, nyq = sr * 0.45;
    for (let i = 0; i < BANDS; i++) { const b = s.bands[i]; if (s.eq_on && b.on) this.eq[i].set(b.shape, clamp(b.freq, 10, nyq), b.q, b.gain_db, sr); else this.eq[i].set(Shape.Peak, 1000, 1, 0, sr); }
    const t = s.sat_on ? s.sat_tone_db : 0; this.tone[0].set(Shape.LowShelf, 700, 0.6, -0.5 * t, sr); this.tone[1].set(Shape.HighShelf, 1400, 0.6, 0.5 * t, sr);
    if (s.sp_on && s.mono_hz > 20.5) this.mono.set(Shape.HighPass, Math.min(s.mono_hz, nyq), 0.707, 0, sr); else this.mono.set(Shape.Peak, 1000, 1, 0, sr);
  }
  process(l, r, s) {
    const n = l.length, sr = this.sr; this.updateFilters(s);
    if (!this.inited) { this.inited = true; for (const b of [...this.eq, ...this.tone]) b.reset(); this.mono.reset(); }
    if (s.lim_on) this.limiter.setCeilingDb(s.ceiling_db); else this.limiter.setOff();
    const inT = dbToGain(s.in_db), outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0, driveT = dbToGain(s.sat_drive_db), makeup = s.sat_auto ? SatCurve.makeup(s.sat_curve, driveT) : 1;
    const satMixT = s.sat_on ? s.sat_mix : 0, spT = s.sp_on ? 1 : 0, midT = dbToGain(s.mid_db), sideT = dbToGain(s.side_db);
    const haasT = clamp(s.haas_ms, -MAX_HAAS_MS, MAX_HAAS_MS) * 0.001 * sr, limGT = s.lim_on ? dbToGain(s.lim_gain_db) : 1;
    if (this.fresh) { this.fresh = false; this.inG.reset(inT); this.outG.reset(outT); this.mix.reset(s.mix); this.bypass.reset(bypT); this.drive.reset(driveT); this.satMix.reset(satMixT);
      this.spAmt.reset(spT); this.width.reset(s.width); this.mid.reset(midT); this.side.reset(sideT); this.pan.reset(s.pan); this.haasD.reset(haasT); this.limG.reset(limGT); }
    const toneActive = !(this.tone[0].isIdentity() && this.tone[1].isIdentity()), eqActive = !this.eq.every(b => b.isIdentity());
    let minGain = 1; const peaks = [0, 0, 0, 0], lat = this.lat, o = this.o;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0;
      this.byp[0].push(xl); this.byp[1].push(xr); const bl = this.byp[0].tapInt(lat), br = this.byp[1].tapInt(lat);
      const g = this.inG.tick(inT), il = xl * g, ir = xr * g; this.dry[0].push(il); this.dry[1].push(ir); const dl = this.dry[0].tapInt(lat), dr = this.dry[1].tapInt(lat);
      const drive = this.drive.tick(driveT), smix = this.satMix.tick(satMixT), amt = this.spAmt.tick(spT), w = this.width.tick(s.width), mg = this.mid.tick(midT), sg = this.side.tick(sideT);
      const pan = this.pan.tick(s.pan), hd = this.haasD.tick(haasT), lg = this.limG.tick(limGT);
      let a = il, b = ir;
      for (const m of s.order) {
        if (m === 0) { this.sat.tick(a, b, s.sat_curve, drive, makeup, smix, o); a = o[0]; b = o[1]; if (toneActive) { this.tone[0].tick(a, b, o); this.tone[1].tick(o[0], o[1], o); a = o[0]; b = o[1]; } }
        else if (m === 1) { if (eqActive) for (const band of this.eq) if (!band.isIdentity()) { band.tick(a, b, o); a = o[0]; b = o[1]; } }
        else if (m === 2) {
          const m0 = 0.5 * (a + b), s0 = 0.5 * (a - b), s1 = this.mono.tickCh(0, s0); this.mono.advance();
          const m1 = m0 * mg, s2 = s1 * sg * w; let y0 = m1 + s2, y1 = m1 - s2;
          this.haas[0].push(y0); this.haas[1].push(y1);
          if (hd < -0.01) y0 = this.haas[0].tapLinear(-hd); else if (hd > 0.01) y1 = this.haas[1].tapLinear(hd);
          balance(y0, y1, pan, o); a += (o[0] - a) * amt; b += (o[1] - b) * amt;
        } else { const gr = this.limiter.tick(a * lg, b * lg, o); if (gr < minGain) minGain = gr; a = o[0]; b = o[1]; }
        const pk = Math.max(Math.abs(a), Math.abs(b)); if (pk > peaks[m]) peaks[m] = pk;
      }
      if (!eqActive) for (const band of this.eq) band.advance();
      if (!toneActive) { this.tone[0].advance(); this.tone[1].advance(); }
      const mx = this.mix.tick(s.mix), og = this.outG.tick(outT), ol = (dl + (a - dl) * mx) * og, or = (dr + (b - dr) * mx) * og;
      this.corr.push(ol, or); const bb = this.bypass.tick(bypT); l[i] = undenorm(ol + (bl - ol) * bb); r[i] = undenorm(or + (br - or) * bb);
    }
    const rel = s.adaptive ? s.release_ms * (1 + 3 * clamp(1 - minGain, 0, 0.5) * 2) : s.release_ms;
    if (Math.abs(rel - this.relFor) > 0.5) { this.relFor = rel; this.limiter.setReleaseMs(sr, rel); }
    return { gr: minGain, peaks, corr: this.corr.value() };
  }
}
function settings(p) {
  const bands = [];
  for (let i = 1; i <= BANDS; i++) {
    let shape = Shape.Peak;
    if (i === 1) shape = [Shape.LowShelf, Shape.HighPass, Shape.Peak][idx(['Low Shelf', 'Low Cut', 'Bell'], p.eq1_type)];
    if (i === 7) shape = [Shape.HighShelf, Shape.LowPass, Shape.Peak][idx(['High Shelf', 'High Cut', 'Bell'], p.eq7_type)];
    bands.push({ on: p[`eq${i}_on`] !== false, shape, freq: p[`eq${i}_freq`], gain_db: p[`eq${i}_gain`], q: p[`eq${i}_q`] });
  }
  return { bypass: !!p.bypass, in_db: p.in_gain, out_db: p.out_gain, mix: p.mix * 0.01, order: ORDERS[idx(ORDER_NAMES, p.order)],
    sat_on: p.sat_on !== false, sat_curve: idx(['Tape', 'Tube', 'Diode'], p.sat_curve), sat_drive_db: p.sat_drive, sat_tone_db: p.sat_tone, sat_mix: p.sat_mix * 0.01, sat_auto: p.sat_auto !== false,
    eq_on: p.eq_on !== false, bands, sp_on: p.sp_on !== false, width: p.sp_width * 0.01, haas_ms: p.sp_haas, mid_db: p.sp_mid, side_db: p.sp_side, pan: p.sp_pan * 0.01, mono_hz: p.sp_mono,
    lim_on: p.lim_on !== false, lim_gain_db: p.lim_gain, ceiling_db: p.lim_ceiling, release_ms: p.lim_release, adaptive: p.lim_adaptive !== false };
}
register('nodaw-racks', sr => new Engine(sr), settings);
