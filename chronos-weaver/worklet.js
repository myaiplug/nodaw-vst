// NoDAW Chronos Weaver — engine.rs ported from the plugin (feat/plugin-suite) to an AudioWorklet.
import { Stft, DelayLine, Noise, Smooth, dbToGain, undenorm, clamp, PI, TAU } from '../shared/dsp.js';
import { register, idx } from '../shared/worklet-base.js';
const B_BANDS = 32, EPS = 1e-7, SNAP_FRAMES = 12;
const M64 = (1n << 64n) - 1n, GOLD = 0x9e3779b97f4a7c15n;
class Weave {
  constructor(n, sr) {
    const bins = n / 2 + 1, hop = n / 4, frames = Math.floor((2.05 * sr) / hop) + 2, v = () => new Float32Array(bins);
    Object.assign(this, { bins, hop, sr, frames, past: [new Float32Array(frames * bins), new Float32Array(frames * bins)], wpos: [0, 0], snap: [v(), v()], snapCount: [0, 0], hasSnap: false,
      frozen: [v(), v()], fphase: [v(), v()], fdelta: [v(), v()], prevPhase: [v(), v()], smear: [v(), v()], gate: [v().fill(1), v().fill(1)], envA: v(), envB: v(),
      freezeAmt: [0, 0], noise: new Noise(0x51ceb00c), fcount: [0n, 0n], warm: [0, 0], fzLive: [false, false], s: null, morph: 0, capturing: false });
  }
  reset() { for (let c = 0; c < 2; c++) { this.past[c].fill(0); this.frozen[c].fill(0); this.fphase[c].fill(0); this.fdelta[c].fill(0); this.prevPhase[c].fill(0); this.smear[c].fill(0); this.gate[c].fill(1);
    this.freezeAmt[c] = 0; this.wpos[c] = 0; this.warm[c] = 0; this.fzLive[c] = false; } }
  static envelope(src, out) { const a = 0.82, n = src.length; let y = src[0]; for (let k = 0; k < n; k++) { y = a * y + (1 - a) * src[k]; out[k] = y; }
    y = out[n - 1]; for (let k = n - 1; k >= 0; k--) { y = a * y + (1 - a) * out[k]; out[k] = Math.max(y, EPS); } }
  idle() { return this.morph <= 0 && !this.s.freeze && this.freezeAmt[0] <= 0 && this.freezeAmt[1] <= 0 && this.s.cloud <= 0; }
  process(c, mag, phase) {
    const bins = this.bins, hopS = this.hop / this.sr, s = this.s, fno = this.fcount[c]; this.fcount[c] = (fno + 1n) & M64;
    const w = this.wpos[c]; this.past[c].set(mag, w * bins); this.wpos[c] = (w + 1) % this.frames;
    if (this.capturing) { const n = this.snapCount[c]; if (n < SNAP_FRAMES) { const t = 1 / (n + 1), sn = this.snap[c]; for (let k = 0; k < bins; k++) sn[k] += (mag[k] - sn[k]) * t; this.snapCount[c] = n + 1; } }
    const target = s.freeze ? 1 : 0, fa = this.freezeAmt[c], step = Math.min(hopS / 0.08, 1), nfa = target > fa ? Math.min(fa + step, 1) : Math.max(fa - step, 0);
    const warm = this.warm[c] >= 4; if (!warm) this.warm[c]++;
    if (fa <= 0 && nfa > 0) this.fzLive[c] = false;
    const fr = this.frozen[c], fp = this.fphase[c], fd = this.fdelta[c], pp = this.prevPhase[c];
    if (nfa > 0 && !this.fzLive[c]) { const full = 0.25 * (2 * (bins - 1)); let peak = 0; for (let k = 0; k < bins; k++) peak = Math.max(peak, mag[k]);
      this.fzLive[c] = warm && peak > full * 1e-4;
      for (let k = 0; k < bins; k++) { fr[k] = mag[k]; fp[k] = phase[k]; let d = phase[k] - pp[k]; d -= TAU * Math.round(d / TAU); fd[k] = d; } }
    this.freezeAmt[c] = nfa; pp.set(phase);
    if (this.idle()) return;
    if (nfa > 0) { const rt = 0.6 + 60 * s.sustain * s.sustain, decay = s.sustain >= 0.999 ? 1 : dbToGain(-60 * hopS / rt);
      for (let k = 0; k < bins; k++) { fr[k] *= decay; fp[k] += fd[k]; if (fp[k] > PI) fp[k] -= TAU * Math.floor((fp[k] + PI) / TAU);
        mag[k] += (fr[k] - mag[k]) * nfa;
        if (nfa >= 1) phase[k] = fp[k]; else { const s1 = Math.sin(phase[k]), c1 = Math.cos(phase[k]), s2 = Math.sin(fp[k]), c2 = Math.cos(fp[k]); phase[k] = Math.atan2(s1 * (1 - nfa) + s2 * nfa, c1 * (1 - nfa) + c2 * nfa); } } }
    const t = this.morph, noB = s.source === 1 && !this.hasSnap;
    if (t > 0 && !noB) {
      const useSnap = s.source === 1 && this.hasSnap, back = clamp(Math.round((s.weave_s * this.sr) / this.hop), 1, this.frames - 1), bf = (this.wpos[c] + this.frames - 1 - back) % this.frames;
      const b = useSnap ? this.snap[c] : this.past[c].subarray(bf * bins, bf * bins + bins);
      if (s.mode === 0) { for (let k = 0; k < bins; k++) { const la = Math.log(mag[k] + EPS), lb = Math.log(b[k] + EPS); mag[k] = Math.max(Math.exp(la + (lb - la) * t) - EPS, 0); } }
      else { Weave.envelope(mag, this.envA); Weave.envelope(b, this.envB); for (let k = 0; k < bins; k++) mag[k] *= Math.pow(clamp(this.envB[k] / this.envA[k], 1e-4, 1e3), t); }
    }
    const cl = s.cloud;
    if (cl > 0) {
      const gs = Math.max(s.grain_s, 0.005), a = 1 - Math.exp(-hopS / gs), dens = clamp(s.density, 0.02, 1), gateA = Math.min(hopS / (0.25 * gs), 1), sc = s.scatter * PI, comp = 1 / Math.sqrt(dens);
      const h = ((fno ^ (BigInt(c) << 62n)) * GOLD) & M64; this.noise = new Noise(Number(((h >> 32n) ^ (h & 0xffffffffn)) & 0xffffffffn));
      const sm = this.smear[c], g = this.gate[c];
      for (let k = 0; k < bins; k++) { sm[k] += (mag[k] - sm[k]) * a; const alive = dens >= 1 || (this.noise.sample() * 0.5 + 0.5) < dens ? 1 : 0;
        g[k] += (alive - g[k]) * gateA; mag[k] += (sm[k] * g[k] * comp - mag[k]) * cl; if (sc > 0) phase[k] += sc * cl * this.noise.sample(); }
    }
  }
}
const lfo = (shape, ph) => { const p = ph - Math.floor(ph); return shape === 1 ? 1 - 4 * Math.abs(p - 0.5) : shape === 2 ? 2 * p - 1 : Math.sin(TAU * p); };
class Engine {
  constructor(sr) { this.sr = sr; this.n = sr > 50000 ? 4096 : 2048; this.stft = new Stft(this.n); this.lat = this.stft.latency(); this.weave = new Weave(this.n, sr);
    this.dry = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)]; this.byp = [new DelayLine(this.lat + 4), new DelayLine(this.lat + 4)];
    this.lastCapture = false; this.outG = new Smooth(1, sr, 20, 20); this.mix = new Smooth(1, sr, 20, 20); this.bypass = new Smooth(0, sr, 10, 10); this.fresh = true; this.o = [0, 0]; }
  command(cmd) { if (cmd === 'clear-snapshot') { const w = this.weave; w.hasSnap = false; w.snap[0].fill(0); w.snap[1].fill(0); w.snapCount = [0, 0]; w.capturing = false; } }
  process(l, r, s, host) {
    const n = l.length, start = host.beats; let morph = s.morph;
    if (s.lfo_depth > 0 && s.lfo_beats > 0) morph += 0.5 * s.lfo_depth * lfo(s.lfo_shape, start / s.lfo_beats);
    morph = clamp(morph, 0, 1); const w = this.weave;
    if (s.capture && !this.lastCapture) { w.snapCount = [0, 0]; w.snap[0].fill(0); w.snap[1].fill(0); w.capturing = true; }
    this.lastCapture = s.capture; let captured = false;
    if (w.capturing && w.snapCount[0] >= SNAP_FRAMES && w.snapCount[1] >= SNAP_FRAMES) { w.capturing = false; w.hasSnap = true; captured = true; }
    w.s = s; w.morph = morph; const outT = dbToGain(s.out_db), bypT = s.bypass ? 1 : 0;
    if (this.fresh) { this.fresh = false; this.outG.reset(outT); this.mix.reset(s.mix); this.bypass.reset(bypT); }
    const o = this.o, lat = this.lat;
    for (let i = 0; i < n; i++) {
      const xl = Number.isFinite(l[i]) ? l[i] : 0, xr = Number.isFinite(r[i]) ? r[i] : 0;
      this.stft.tick(xl, xr, w, o); const g = this.outG.tick(outT), a = o[0] * g, b = o[1] * g;
      this.dry[0].push(xl); this.dry[1].push(xr); const dl = this.dry[0].tapInt(lat), dr = this.dry[1].tapInt(lat), m = this.mix.tick(s.mix);
      const ol = m >= 1 ? a : dl + (a - dl) * m, or = m >= 1 ? b : dr + (b - dr) * m;
      this.byp[0].push(xl); this.byp[1].push(xr); const bp = this.bypass.tick(bypT); let yl = ol, yr = or;
      if (bp > 0) { const bl = this.byp[0].tapInt(lat), br = this.byp[1].tapInt(lat); if (bp >= 1) { yl = bl; yr = br; } else { yl = ol + (bl - ol) * bp; yr = or + (br - or) * bp; } }
      l[i] = undenorm(yl); r[i] = undenorm(yr);
    }
    const bins = w.bins, norm = 2 / (0.5 * this.n), back = clamp(Math.round((s.weave_s * this.sr) / w.hop), 1, w.frames - 1), bf = (w.wpos[0] + w.frames - 1 - back) % w.frames;
    const useSnap = s.source === 1 && w.hasSnap, bdb = new Array(B_BANDS).fill(-120);
    if (s.source === 0 || useSnap) for (let j = 0; j < B_BANDS; j++) {
      const f0 = 20 * Math.pow(1000, j / B_BANDS), f1 = 20 * Math.pow(1000, (j + 1) / B_BANDS), k0 = Math.floor(f0 / this.sr * 2 * (bins - 1)), k1 = Math.min(Math.max(Math.floor(f1 / this.sr * 2 * (bins - 1)), k0 + 1), bins);
      let m = 0; for (let k = Math.min(k0, bins - 1); k < k1; k++) m = Math.max(m, useSnap ? w.snap[0][k] : w.past[0][bf * bins + k]); bdb[j] = 20 * Math.log10(m * norm + 1e-9); }
    return { morph, freeze: 0.5 * (w.freezeAmt[0] + w.freezeAmt[1]), hasSnapshot: w.hasSnap, captured, bdb };
  }
}
function settings(p) {
  const bar = 4, ls = idx(['Off', '1/4', '1/2', '1 Bar', '2 Bars', '4 Bars'], p.lfo_sync, 3);
  return { bypass: !!p.bypass, morph: p.morph * 0.01, mode: idx(['Blend', 'Cross'], p.morph_mode), source: idx(['Past', 'Snapshot'], p.source), weave_s: p.weave * 0.001,
    capture: !!p.capture, freeze: !!p.freeze, sustain: p.sustain * 0.01, cloud: p.cloud * 0.01, grain_s: p.grain * 0.001, density: p.density * 0.01, scatter: p.scatter * 0.01,
    lfo_depth: p.lfo_depth * 0.01, lfo_beats: [0, 1, 0.5 * bar, bar, 2 * bar, 4 * bar][ls] || 0, lfo_shape: idx(['Sine', 'Triangle', 'Ramp'], p.lfo_shape), out_db: p.out_gain, mix: p.mix * 0.01 };
}
register('nodaw-chronos-weaver', sr => new Engine(sr), settings);
