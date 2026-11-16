// NoDAW Harmonic Space — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { EqBand, Shape, FeedbackDelay, Reverb, Flanger, Correlation, width, Smooth, dbToGain, undenorm, clamp } from '../shared/dsp.js';
import { register } from '../shared/worklet-base.js';
const BANDS = 7, MUTE_DB = -36, HPF_OFF = 20.5, LPF_OFF = 21900;
const bandShape = i => (i === 0 ? Shape.LowShelf : i === 6 ? Shape.HighShelf : Shape.Peak);
class Engine {
  constructor(sr) {
    this.sr = sr; const ramp = Math.floor(sr * 0.02), sm = (v, ms) => new Smooth(v, sr, ms, ms);
    this.eq = Array.from({ length: BANDS }, () => new EqBand(ramp)); this.solo = Array.from({ length: BANDS }, () => new EqBand(ramp));
    this.sub = new EqBand(ramp); this.hpf = new EqBand(ramp); this.lpf = new EqBand(ramp);
    this.delay = new FeedbackDelay(sr); this.reverb = new Reverb(sr); this.flanger = new Flanger(sr); this.revKey = ''; this.fxLive = [false, false, false]; this.corr = new Correlation(sr);
    this.amp = sm(1, 15); this.out = sm(1, 15); this.mix = sm(1, 20); this.bypass = sm(0, 8); this.soloAmt = sm(0, 10); this.dmix = sm(0, 25); this.rmix = sm(0, 25); this.fmix = sm(0, 25); this.dfb = sm(0.3, 30); this.width = sm(1, 20);
    this.fresh = true; this.o = [0, 0]; this.o2 = [0, 0];
  }
  all() { return [...this.eq, ...this.solo, this.sub, this.hpf, this.lpf]; }
  update(s) {
    const sr = this.sr, nyq = sr * 0.45;
    s.bands.forEach((b, i) => this.eq[i].set(bandShape(i), clamp(b.freq, 10, nyq), b.q, b.mute ? MUTE_DB : b.gain_db, sr));
    s.bands.forEach((b, i) => this.solo[i].set(i === 0 ? Shape.LowPass : i === 6 ? Shape.HighPass : Shape.BandPass, clamp(b.freq, 10, nyq), Math.max(b.q, 0.5), 0, sr));
    this.sub.set(Shape.LowShelf, clamp(s.sub_hz, 20, 400), 0.8, s.sub_db, sr);
    if (s.hpf_hz > HPF_OFF) this.hpf.set(Shape.HighPass, Math.min(s.hpf_hz, nyq), 0.707, 0, sr); else this.hpf.set(Shape.Peak, 1000, 1, 0, sr);
    if (s.lpf_hz < LPF_OFF && s.lpf_hz < nyq) this.lpf.set(Shape.LowPass, s.lpf_hz, 0.707, 0, sr); else this.lpf.set(Shape.Peak, 1000, 1, 0, sr);
    const rp = { decay: s.decay_s, size: s.size, dampHz: s.damp_hz, predelayMs: s.predelay_ms }, k = JSON.stringify(rp);
    if (k !== this.revKey) { this.revKey = k; this.reverb.set(rp); }
    this.delay.setTone(Math.min(s.delay_tone_hz, nyq), 60);
  }
  process(l, r, s) {
    const n = l.length; this.update(s);
    const ampT = dbToGain(s.amp_db), outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0, anySolo = s.bands.some(b => b.solo), soloT = anySolo ? 1 : 0, fbT = clamp(s.delay_fb, 0, 0.9);
    if (this.fresh) { this.fresh = false; for (const b of this.all()) b.reset(); this.delay.reset(s.delay_s); this.reverb.snap();
      this.amp.reset(ampT); this.out.reset(outT); this.mix.reset(s.mix); this.bypass.reset(bypT); this.soloAmt.reset(soloT); this.dmix.reset(s.delay_mix); this.rmix.reset(s.reverb_mix); this.fmix.reset(s.flanger_mix); this.dfb.reset(fbT); this.width.reset(s.width); }
    const targets = [s.delay_mix, s.reverb_mix, s.flanger_mix], shown = [this.dmix.value, this.rmix.value, this.fmix.value];
    for (let k = 0; k < 3; k++) { const live = targets[k] > 0 || shown[k] > 0; if (!live && this.fxLive[k]) { if (k === 0) this.delay.reset(s.delay_s); else if (k === 1) this.reverb.reset(); else this.flanger.reset(); } this.fxLive[k] = live; }
    const eqActive = !this.eq.every(b => b.isIdentity()), soloActive = anySolo || this.soloAmt.value > 0, toneActive = !(this.sub.isIdentity() && this.hpf.isIdentity() && this.lpf.isIdentity());
    const fxPeak = [0, 0, 0], o = this.o, o2 = this.o2;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0, g = this.amp.tick(ampT);
      let a = xl * g, b = xr * g;
      for (const band of this.eq) { if (eqActive && !band.isIdentity()) { band.tick(a, b, o); a = o[0]; b = o[1]; } else band.advance(); }
      const sa = this.soloAmt.tick(soloT);
      if (soloActive) { let ol = 0, or = 0; this.solo.forEach((band, k) => { if (s.bands[k].solo) { band.tick(a, b, o); ol += o[0]; or += o[1]; } else band.advance(); }); a += (ol - a) * sa; b += (or - b) * sa; }
      else for (const band of this.solo) band.advance();
      if (toneActive) { this.sub.tick(a, b, o); this.hpf.tick(o[0], o[1], o); this.lpf.tick(o[0], o[1], o); a = o[0]; b = o[1]; } else { this.sub.advance(); this.hpf.advance(); this.lpf.advance(); }
      let wl = 0, wr = 0; const dm = this.dmix.tick(s.delay_mix), fbk = this.dfb.tick(fbT);
      if (this.fxLive[0]) { this.delay.tick(a, b, s.delay_s, fbk, s.delay_pp, o2); const y0 = o2[0] * dm, y1 = o2[1] * dm; fxPeak[0] = Math.max(fxPeak[0], Math.abs(y0), Math.abs(y1)); wl += y0; wr += y1; }
      const rm = this.rmix.tick(s.reverb_mix);
      if (this.fxLive[1]) { this.reverb.tick(a, b, o2); const y0 = o2[0] * rm, y1 = o2[1] * rm; fxPeak[1] = Math.max(fxPeak[1], Math.abs(y0), Math.abs(y1)); wl += y0; wr += y1; }
      const fm = this.fmix.tick(s.flanger_mix);
      if (this.fxLive[2]) { this.flanger.tick(a, b, s.fl_rate, s.fl_depth, s.fl_fb, o2); const y0 = o2[0] * fm, y1 = o2[1] * fm; fxPeak[2] = Math.max(fxPeak[2], Math.abs(y0), Math.abs(y1)); wl += y0; wr += y1; }
      width(a + wl, b + wr, this.width.tick(s.width), o);
      const og = this.out.tick(outT), mx = this.mix.tick(s.mix), ol = xl + (o[0] * og - xl) * mx, or = xr + (o[1] * og - xr) * mx;
      this.corr.push(ol, or); const bb = this.bypass.tick(bypT); l[i] = undenorm(ol + (xl - ol) * bb); r[i] = undenorm(or + (xr - or) * bb);
    }
    return { fx: fxPeak, corr: this.corr.value() };
  }
}
function settings(p) {
  const bands = []; for (let i = 1; i <= BANDS; i++) bands.push({ freq: p[`b${i}_freq`], gain_db: p[`b${i}_gain`], q: p[`b${i}_q`], solo: !!p[`b${i}_solo`], mute: !!p[`b${i}_mute`] });
  return { bypass: !!p.bypass, amp_db: p.amp, hpf_hz: p.hpf, lpf_hz: p.lpf, bands, sub_db: p.sub, sub_hz: p.sub_freq, delay_mix: p.dly_mix * 0.01, delay_s: p.dly_time * 0.001,
    delay_fb: p.dly_fb * 0.01, delay_pp: !!p.dly_pp, delay_tone_hz: p.dly_tone, reverb_mix: p.rev_mix * 0.01, decay_s: p.rev_decay, size: p.rev_size * 0.01, predelay_ms: p.rev_pre,
    damp_hz: p.rev_damp, flanger_mix: p.fl_mix * 0.01, fl_rate: p.fl_rate, fl_depth: p.fl_depth * 0.01, fl_fb: p.fl_fb * 0.01, width: p.width * 0.01, out_db: p.out_gain, mix: p.mix * 0.01 };
}
register('nodaw-harmonic-space', sr => new Engine(sr), settings);
