// NoDAW Screw — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { DelayLine, Biquad, Coeffs, Limiter, PitchShifter, Reverb, Smooth, Wobble, dbToGain, semitonesToRatio, excursionMs, clamp,
  CHOP_PATTERNS, CHOP_RATES, chopStepAt, chopLevelGain } from '../shared/dsp.js';
import { register, idx } from '../shared/worklet-base.js';

const SHELF_HZ = 150;
class Engine {
  constructor(sr) {
    this.sr = sr; this.pitch = new PitchShifter(sr); this.wobble = new Wobble(sr); this.reverb = new Reverb(sr); this.limiter = new Limiter(sr, 2.0, 80.0);
    this.latWet = this.pitch.latency() + this.wobble.latency(); this.latTotal = this.latWet + this.limiter.latency();
    this.dry = [new DelayLine(this.latWet + 4), new DelayLine(this.latWet + 4)]; this.byp = [new DelayLine(this.latTotal + 4), new DelayLine(this.latTotal + 4)];
    this.lp = [new Biquad(), new Biquad()]; this.shelf = [new Biquad(), new Biquad()]; this.lpC = Coeffs.IDENTITY; this.shelfC = Coeffs.IDENTITY; this.lpFor = -1; this.shelfFor = NaN;
    const sm = (v, a, b = a) => new Smooth(v, sr, a, b);
    this.inG = sm(1, 15); this.outG = sm(1, 15); this.mix = sm(1, 20); this.rev = sm(0, 20); this.width = sm(1, 20); this.bypass = sm(0, 8); this.chopG = sm(1, 1.5, 3);
    this.fresh = true; this.o = [0, 0]; this.o2 = [0, 0]; this.o3 = [0, 0];
  }
  updateTone(s) {
    const lp = clamp(s.lowpass_hz, 200, 20000);
    if (lp !== this.lpFor) { this.lpFor = lp; this.lpC = lp >= 19900 ? Coeffs.IDENTITY : Coeffs.lowpass(Math.min(lp, this.sr * 0.45), 0.707, this.sr); }
    if (s.bass_db !== this.shelfFor) { this.shelfFor = s.bass_db; this.shelfC = Math.abs(s.bass_db) < 0.01 ? Coeffs.IDENTITY : Coeffs.lowShelf(SHELF_HZ, s.bass_db, this.sr); }
  }
  process(l, r, s, host) {
    const n = l.length; this.updateTone(s); this.reverb.set(s.reverb);
    if (s.limiter_on) this.limiter.setCeilingDb(s.ceiling_db); else this.limiter.setOff();
    const ratio = semitonesToRatio(s.pitch_st), rate = clamp(s.wow_rate_hz, 0.05, 20), exc = s.wow_cents > 0 ? excursionMs(s.wow_cents, rate) : 0;
    const start = host.beats, inc = host.tempo / (60 * this.sr);
    const chopOn = s.pattern.some(v => v !== 2) && s.chop_depth > 0;
    const inT = dbToGain(s.in_db), outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0;
    if (this.fresh) { this.fresh = false; this.inG.reset(inT); this.outG.reset(outT); this.mix.reset(s.mix); this.rev.reset(s.rev_mix); this.width.reset(s.width); this.bypass.reset(bypT); }
    let minGain = 1, step = 0; const latWet = this.latWet, latTotal = this.latTotal, o = this.o, o2 = this.o2, o3 = this.o3;
    const toneOff = () => this.lpFor >= 19900 && Math.abs(s.bass_db) < 0.01;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0;
      this.byp[0].push(xl); this.byp[1].push(xr); const bl = this.byp[0].tapInt(latTotal), br = this.byp[1].tapInt(latTotal);
      const g = this.inG.tick(inT), il = xl * g, ir = xr * g;
      this.dry[0].push(il); this.dry[1].push(ir); const dl = this.dry[0].tapInt(latWet), dr = this.dry[1].tapInt(latWet);
      this.pitch.tick(il, ir, ratio, o); this.wobble.tick(o[0], o[1], exc, rate, s.flutter, o2);
      let tl = o2[0], tr = o2[1];
      if (!toneOff()) { tl = this.shelf[0].tick(this.lp[0].tick(tl, this.lpC), this.shelfC); tr = this.shelf[1].tick(this.lp[1].tick(tr, this.lpC), this.shelfC); }
      const pos = start + inc * (i - latWet); step = chopStepAt(pos, s.chop_spb);
      const cg = this.chopG.tick(chopOn ? chopLevelGain(s.pattern[step], s.chop_depth) : 1), cl = tl * cg, cr = tr * cg;
      this.reverb.tick(cl, cr, o3); const rm = this.rev.tick(s.rev_mix); let wl = cl + rm * o3[0], wr = cr + rm * o3[1];
      const w = this.width.tick(s.width), m = 0.5 * (wl + wr), side = 0.5 * (wl - wr) * w; wl = m + side; wr = m - side;
      const mx = this.mix.tick(s.mix), og = this.outG.tick(outT);
      const gr = this.limiter.tick((dl + (wl - dl) * mx) * og, (dr + (wr - dr) * mx) * og, o); if (gr < minGain) minGain = gr;
      const b = this.bypass.tick(bypT); l[i] = o[0] + (bl - o[0]) * b; r[i] = o[1] + (br - o[1]) * b;
    }
    return { gr: minGain, step };
  }
}
// 'Custom' (index 8) reads the drawn pattern, which starts as Trap Roll like the plugin's shared state.
const PATTERNS = CHOP_PATTERNS.map(p => p[0]), RATES = CHOP_RATES.map(r => r[0]);
function settings(p) {
  return {
    bypass: !!p.bypass, in_db: p.in_gain, out_db: p.out_gain, mix: p.mix * 0.01, pitch_st: p.pitch, wow_cents: p.wow, wow_rate_hz: p.wow_rate, flutter: p.flutter * 0.01,
    lowpass_hz: p.lowpass, bass_db: p.bass, rev_mix: p.rev_mix * 0.01, reverb: { decay: p.rev_decay, size: p.rev_size, dampHz: p.rev_damp, predelayMs: p.rev_pre },
    pattern: idx(PATTERNS.concat('Custom'), p.chop_pattern) >= PATTERNS.length ? (Array.isArray(p.custom_pattern) && p.custom_pattern.length === 16 ? p.custom_pattern.map(v => Math.min(2, Math.max(0, v | 0))) : CHOP_PATTERNS[5][1]) : CHOP_PATTERNS[idx(PATTERNS, p.chop_pattern)][1], chop_spb: CHOP_RATES[idx(RATES, p.chop_rate, 1)][1], chop_depth: p.chop_depth * 0.01,
    width: p.width * 0.01, limiter_on: p.limiter !== false, ceiling_db: p.ceiling,
  };
}
register('nodaw-screw', sr => new Engine(sr), settings);
