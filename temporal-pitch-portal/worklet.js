// NoDAW Temporal Pitch Portal — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { TimeWarp, PitchShifter, EqBand, Shape, FeedbackDelay, DelayLine, Noise, Smooth, dbToGain, undenorm, clamp, semitonesToRatio, TAU } from '../shared/dsp.js';
import { register, idx } from '../shared/worklet-base.js';
const TILT_LO = 250, TILT_HI = 2500, TILT_DB = 6;
const syncBeats = (i, bar) => (i === 0 ? 1 : i === 1 ? 0.5 * bar : i === 3 ? 2 * bar : bar);
class Engine {
  constructor(sr) {
    this.sr = sr; this.warp = new TimeWarp(sr); this.pitch = new PitchShifter(sr); this.lat = this.warp.latency() + this.pitch.latency();
    this.lo = new EqBand(64); this.hi = new EqBand(64); this.echo = new FeedbackDelay(sr);
    this.dry = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)]; this.byp = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)];
    this.lastPeriod = -Infinity; this.st = new Smooth(0, sr, 30, 30); this.driftPhase = 0; this.driftNoise = new Noise(0x7e3a11d5); this.driftRand = 0; this.driftTarget = 0; this.driftCount = 0;
    this.outG = new Smooth(1, sr, 20, 20); this.mix = new Smooth(1, sr, 20, 20); this.bypass = new Smooth(0, sr, 10, 10); this.echoAmt = new Smooth(0, sr, 20, 20); this.fresh = true;
    this.o = [0, 0]; this.o2 = [0, 0];
  }
  process(l, r, s, host) {
    const n = l.length, sr = this.sr, t = clamp(s.tone, -1, 1);
    this.lo.set(Shape.LowShelf, TILT_LO, 0.7, -t * TILT_DB, sr); this.hi.set(Shape.HighShelf, TILT_HI, 0.7, t * TILT_DB, sr); this.echo.setTone(Math.min(9000, sr * 0.45), 90);
    const glide = Math.max(s.glide_ms, 1); this.st.setTimes(sr, glide, glide);
    const outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0, echoT = clamp(s.echo_mix, 0, 1);
    if (this.fresh) { this.fresh = false; this.st.reset(s.pitch_st); this.outG.reset(outT); this.mix.reset(s.mix); this.bypass.reset(bypT); this.echoAmt.reset(echoT); this.echo.reset(s.echo_s); }
    const start = host.beats, inc = host.tempo / (60 * sr), perBeats = syncBeats(s.sync, 4);
    const period = clamp(Math.round(perBeats / inc), 16, this.warp.maxPeriod), driftInc = clamp(s.drift_hz, 0.01, 20) / sr;
    const driftHold = Math.max(Math.floor(sr / (Math.max(s.drift_hz, 0.01) * 1.7)), 1), driftOn = s.drift_cents > 0, rate = clamp(s.time, 0.5, 2);
    let ratio = 1, driftC = 0, echoPeak = 0; const o = this.o, o2 = this.o2;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0;
      const beat = start + inc * i, id = Math.floor(beat / perBeats), newPeriod = id !== this.lastPeriod; this.lastPeriod = id;
      this.warp.tick(xl, xr, rate, s.stretch, newPeriod, period, o);
      const st = this.st.tick(s.pitch_st);
      if (driftOn) { this.driftPhase += driftInc; if (this.driftPhase >= 1) this.driftPhase -= 1;
        if (this.driftCount === 0) { this.driftTarget = this.driftNoise.sample(); this.driftCount = driftHold; } this.driftCount--;
        this.driftRand += (this.driftTarget - this.driftRand) * Math.min(8 / driftHold, 1); driftC = s.drift_cents * (0.65 * Math.sin(TAU * this.driftPhase) + 0.35 * this.driftRand); }
      else driftC = 0;
      ratio = st === 0 && driftC === 0 ? 1 : semitonesToRatio(st + driftC * 0.01);
      this.pitch.tick(o[0], o[1], ratio, o); this.hi.tick(o[0], o[1], o); this.lo.tick(o[0], o[1], o); let a = o[0], b = o[1];
      const ea = this.echoAmt.tick(echoT); let el = 0, er = 0; if (ea > 0 || echoT > 0) { this.echo.tick(a, b, s.echo_s, s.echo_fb, false, o2); el = o2[0]; er = o2[1]; }
      echoPeak = Math.max(echoPeak, Math.abs(el), Math.abs(er)); a += ea * el; b += ea * er; const g = this.outG.tick(outT); a *= g; b *= g;
      this.dry[0].push(xl); this.dry[1].push(xr); const dl = this.dry[0].tapInt(this.lat), dr = this.dry[1].tapInt(this.lat), m = this.mix.tick(s.mix);
      const ol = m >= 1 ? a : dl + (a - dl) * m, or = m >= 1 ? b : dr + (b - dr) * m;
      this.byp[0].push(xl); this.byp[1].push(xr); const bp = this.bypass.tick(bypT); let yl = ol, yr = or;
      if (bp > 0) { const bl = this.byp[0].tapInt(this.lat), br = this.byp[1].tapInt(this.lat); yl = ol + (bl - ol) * bp; yr = or + (br - or) * bp; }
      l[i] = undenorm(yl); r[i] = undenorm(yr);
    }
    if (echoT <= 0 && this.echoAmt.value <= 0) this.echo.reset(s.echo_s);
    const beatEnd = start + inc * n, ph = beatEnd / perBeats;
    return { phase: ph - Math.floor(ph), pitch: this.st.value + driftC * 0.01, drift: driftC, ripple: ratio * (s.stretch ? 1 : rate), echo: echoPeak };
  }
}
function settings(p) {
  return { bypass: !!p.bypass, time: p.time * 0.01, stretch: idx(['Stretch', 'Tape'], p.warp_mode) === 0, sync: idx(['1/4', '1/2 Bar', '1 Bar', '2 Bars'], p.warp_sync, 2),
    pitch_st: p.pitch, glide_ms: p.glide, drift_cents: p.drift, drift_hz: p.drift_rate, tone: p.tone * 0.01, echo_mix: p.echo_mix * 0.01, echo_s: p.echo_time * 0.001, echo_fb: p.echo_fb * 0.01,
    out_db: p.out_gain, mix: p.mix * 0.01 };
}
register('nodaw-temporal-pitch-portal', sr => new Engine(sr), settings);
