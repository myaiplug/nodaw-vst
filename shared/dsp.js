// NoDAW DSP, ported to JavaScript from the Rust crate `nodaw-dsp` (feat/plugin-suite).
// Runs inside AudioWorklet processors. Ported module by module; names follow the Rust code.
export const PI = Math.PI, TAU = 2 * Math.PI;
export const dbToGain = db => Math.pow(10, db * 0.05);
export const gainToDb = g => 20 * Math.log10(Math.max(Math.abs(g), 1e-9));
export const undenorm = x => (Math.abs(x) < 1e-20 ? 0 : x);
export const clamp = (x, a, b) => (x < a ? a : x > b ? b : x);
export const nextPow2 = n => { let p = 1; while (p < n) p <<= 1; return p; };
export const semitonesToRatio = st => Math.pow(2, st / 12);

export class Noise {
  constructor(seed) { this.s = (seed || 0x9e3779b9) >>> 0; }
  sample() { let x = this.s; x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0; this.s = x; return x * (2 / 4294967296) - 1; }
}

// util::Smooth: one-pole with separate up/down times, lands exactly on the target.
export class Smooth {
  constructor(value, sr, upMs, downMs = upMs) { this.value = value; this.setTimes(sr, upMs, downMs); }
  setTimes(sr, upMs, downMs) { const c = ms => 1 - Math.exp(-1 / (Math.max(ms, 0.01) * 0.001 * sr)); this.up = c(upMs); this.down = c(downMs); }
  reset(v) { this.value = v; }
  tick(t) {
    const c = t > this.value ? this.up : this.down;
    const n = this.value + (t - this.value) * c;
    this.value = (n === this.value || Math.abs(t - n) < 3e-5) ? t : n;
    return this.value;
  }
}

// filter::Coeffs / Biquad (RBJ cookbook, transposed direct form II)
export const Coeffs = {
  IDENTITY: { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 },
  norm(b0, b1, b2, a0, a1, a2) { return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }; },
  lowpass(fc, q, sr) { const w = TAU * clamp(fc, 10, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w), al = s / (2 * q); return this.norm((1 - c) / 2, 1 - c, (1 - c) / 2, 1 + al, -2 * c, 1 - al); },
  highpass(fc, q, sr) { const w = TAU * clamp(fc, 5, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w), al = s / (2 * q); return this.norm((1 + c) / 2, -(1 + c), (1 + c) / 2, 1 + al, -2 * c, 1 - al); },
  bandpass(fc, q, sr) { const w = TAU * clamp(fc, 10, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w), al = s / (2 * q); return this.norm(al, 0, -al, 1 + al, -2 * c, 1 - al); },
  lowShelf(fc, db, sr, slopeQ = Math.SQRT1_2) { const a = Math.pow(10, db / 40), w = TAU * clamp(fc, 10, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w); const al = s / (2 * slopeQ), sq = 2 * Math.sqrt(a) * al;
    return this.norm(a * ((a + 1) - (a - 1) * c + sq), 2 * a * ((a - 1) - (a + 1) * c), a * ((a + 1) - (a - 1) * c - sq), (a + 1) + (a - 1) * c + sq, -2 * ((a - 1) + (a + 1) * c), (a + 1) + (a - 1) * c - sq); },
  highShelf(fc, db, sr, slopeQ = Math.SQRT1_2) { const a = Math.pow(10, db / 40), w = TAU * clamp(fc, 10, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w); const al = s / (2 * slopeQ), sq = 2 * Math.sqrt(a) * al;
    return this.norm(a * ((a + 1) + (a - 1) * c + sq), -2 * a * ((a - 1) + (a + 1) * c), a * ((a + 1) + (a - 1) * c - sq), (a + 1) - (a - 1) * c + sq, 2 * ((a - 1) - (a + 1) * c), (a + 1) - (a - 1) * c - sq); },
  peak(fc, q, db, sr) { const a = Math.pow(10, db / 40), w = TAU * clamp(fc, 10, sr * 0.49) / sr, s = Math.sin(w), c = Math.cos(w), al = s / (2 * Math.max(q, 0.05));
    return this.norm(1 + al * a, -2 * c, 1 - al * a, 1 + al / a, -2 * c, 1 - al / a); },
};
export class Biquad {
  constructor() { this.z1 = 0; this.z2 = 0; }
  tick(x, c) { const y = c.b0 * x + this.z1; this.z1 = undenorm(c.b1 * x - c.a1 * y + this.z2); this.z2 = undenorm(c.b2 * x - c.a2 * y); return y; }
  reset() { this.z1 = this.z2 = 0; }
}

// delay::DelayLine / Allpass
export class DelayLine {
  constructor(maxDelay) { const n = nextPow2(maxDelay + 8); this.buf = new Float32Array(n); this.mask = n - 1; this.pos = 0; }
  clear() { this.buf.fill(0); }
  push(x) { this.buf[this.pos] = x; this.pos = (this.pos + 1) & this.mask; }
  at(back) { return this.buf[(this.pos - 1 - back) & this.mask]; }
  tapInt(d) { return this.at(d); }
  tapLinear(d) { const i = d | 0, f = d - i, a = this.at(i), b = this.at(i + 1); return a + (b - a) * f; }
  tapHermite(d) { const i = d | 0, f = d - i; const ym1 = this.at(i - 1), y0 = this.at(i), y1 = this.at(i + 1), y2 = this.at(i + 2);
    const c1 = 0.5 * (y1 - ym1), c2 = ym1 - 2.5 * y0 + 2 * y1 - 0.5 * y2, c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1); return ((c3 * f + c2) * f + c1) * f + y0; }
}
export class Allpass {
  constructor(len, g) { this.line = new DelayLine(len + 2); this.len = Math.max(len, 1); this.g = g; }
  tick(x) { const d = this.line.tapInt(this.len - 1), v = x - this.g * d; this.line.push(v); return undenorm(d + this.g * v); }
  clear() { this.line.clear(); }
}

// chop: 16-step gate patterns
export const CHOP_STEPS = 16;
export const CHOP_PATTERNS = [
  ['Off', Array(16).fill(2)], ['Pulse', [2,0,2,0,2,0,2,0,2,0,2,0,2,0,2,0]], ['Offbeat', [0,0,2,0,0,0,2,0,0,0,2,0,0,0,2,0]],
  ['Skip', [2,2,0,2,2,0,2,0,2,2,0,2,2,0,0,2]], ['Stutter', [2,2,2,2,0,0,2,2,2,2,2,2,0,0,2,0]], ['Trap Roll', [2,0,1,0,2,0,1,1,2,0,1,0,2,1,2,1]],
  ['Half-time', [2,1,0,0,2,1,0,0,2,1,0,0,2,1,0,0]], ['Gallop', [2,0,2,2,0,2,0,2,2,0,2,2,0,2,0,0]],
];
export const CHOP_RATES = [['1/4', 1], ['1/8', 2], ['1/16', 4], ['1/32', 8], ['1/8T', 3], ['1/16T', 6]];
export const chopStepAt = (beats, spb) => { const s = Math.floor(beats * spb); return ((s % 16) + 16) % 16; };
export const chopLevelGain = (level, depth) => 1 - depth * (1 - (level === 0 ? 0 : level === 1 ? 0.5 : 1));

// pitch::PitchShifter — SOLA varispeed reader with correlation-searched jumps and a crossfade
export class PitchShifter {
  constructor(sr) {
    const ms = t => Math.max(Math.round(t * 0.001 * sr), 8);
    this.corrLen = ms(5.33); this.xfLen = ms(8.0); this.search = ms(7.5);
    this.lagFloor = this.corrLen + this.xfLen + 16; this.lagMax = this.lagFloor + 2 * this.search + ms(10.0);
    this.lagMean = (this.lagFloor + this.lagMax) >> 1;
    const size = nextPow2(this.lagMax + this.corrLen + 2 * this.search + this.xfLen + 64);
    this.l = new Float32Array(size); this.r = new Float32Array(size); this.mono = new Float32Array(size); this.mask = size - 1;
    this.scratch = new Float32Array(this.corrLen); this.active = new Smooth(0, sr, 15, 15); this.reset();
  }
  latency() { return this.lagMean; }
  reset() { this.l.fill(0); this.r.fill(0); this.mono.fill(0);
    this.n = this.lagMax + this.corrLen + 2 * this.search + this.xfLen + 32; this.rp = this.n - 1 - this.lagMean; this.rp2 = this.rp; this.xf = 0; this.active.reset(0); }
  at(b, i) { return b[i & this.mask]; }
  read(b, pos) { const i = Math.floor(pos), f = pos - i; const ym1 = b[(i - 1) & this.mask], y0 = b[i & this.mask], y1 = b[(i + 1) & this.mask], y2 = b[(i + 2) & this.mask];
    const c1 = 0.5 * (y1 - ym1), c2 = ym1 - 2.5 * y0 + 2 * y1 - 0.5 * y2, c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1); return ((c3 * f + c2) * f + c1) * f + y0; }
  bestJump(nominal) {
    const newest = this.n - 1, start = Math.floor(this.rp), m = this.mono, mk = this.mask, cl = this.corrLen, sc = this.scratch;
    for (let k = 0; k < cl; k++) sc[k] = m[(start + k) & mk];
    const lo = Math.max(Math.max(nominal - this.search, 0), this.lagFloor), hi = Math.min(nominal + this.search, this.lagMax);
    const score = lag => { const b0 = newest - lag; let dot = 0, en = 1e-6; for (let k = 0; k < cl; k++) { const b = m[(b0 + k) & mk]; dot += sc[k] * b; en += b * b; } return dot / Math.sqrt(en); };
    let best = clamp(nominal, lo, hi), bs = score(best);
    for (let lag = lo; lag <= hi; lag++) { const s = score(lag); if (s > bs) { bs = s; best = lag; } }
    return best;
  }
  tick(l, r, ratio, out) {
    ratio = clamp(ratio, 0.5, 2.0);
    const w = this.n & this.mask; this.l[w] = l; this.r[w] = r; this.mono[w] = 0.5 * (l + r); this.n++;
    if (this.xf === 0) { const lag = this.n - 1 - this.rp; const down = lag > this.lagMax, up = lag < this.lagFloor;
      if (down || up) { const nominal = down ? this.lagFloor + this.search : this.lagMax - this.search; const best = this.bestJump(nominal);
        this.rp2 = (this.n - 1) - best + (this.rp - Math.floor(this.rp)); this.xf = 1; } }
    let yl = this.read(this.l, this.rp), yr = this.read(this.r, this.rp); this.rp += ratio;
    if (this.xf > 0) { const g = 0.5 - 0.5 * Math.cos(PI * this.xf / this.xfLen); yl += (this.read(this.l, this.rp2) - yl) * g; yr += (this.read(this.r, this.rp2) - yr) * g;
      this.rp2 += ratio; this.xf++; if (this.xf > this.xfLen) { this.xf = 0; this.rp = this.rp2; } }
    const a = this.active.tick(Math.min(Math.abs(ratio - 1) / 0.004, 1));
    const straight = this.n - 1 - this.lagMean, sl = this.l[straight & this.mask], sr = this.r[straight & this.mask];
    if (a === 0) { out[0] = sl; out[1] = sr; } else { out[0] = sl + a * (yl - sl); out[1] = sr + a * (yr - sr); }
  }
}

// wobble::Wobble — wow (sine, R channel phase offset 0.19) + flutter (filtered noise) on a modulated delay
export const WOBBLE_CENTRE_S = 0.008, WOBBLE_MAX_S = 0.007;
export const excursionMs = (cents, rate) => Math.min((Math.pow(2, cents / 1200) - 1) / (TAU * Math.max(rate, 0.01)) * 1000, WOBBLE_MAX_S * 1000);
export class Wobble {
  constructor(sr) { const centre = Math.round(WOBBLE_CENTRE_S * sr); this.l = new DelayLine(centre * 2 + 8); this.r = new DelayLine(centre * 2 + 8); this.centre = centre; this.sr = sr;
    this.flutCoef = 1 - Math.exp(-TAU * 9 / sr); this.reset(); }
  latency() { return this.centre; }
  reset() { this.l.clear(); this.r.clear(); this.phase = 0; this.fl = 0; this.fr = 0; this.nl = new Noise(0x12345678); this.nr = new Noise(0x87654321); }
  tick(l, r, excMs, rate, flutter, out) {
    this.l.push(l); this.r.push(r); this.phase += rate / this.sr; this.phase -= Math.floor(this.phase);
    const wl = Math.sin(TAU * this.phase), wr = Math.sin(TAU * (this.phase + 0.19));
    this.fl += (this.nl.sample() - this.fl) * this.flutCoef; this.fr += (this.nr.sample() - this.fr) * this.flutCoef;
    const m = this.sr * 0.001, fl = flutter * 0.18 * m * 4, ex = excMs * m, max = WOBBLE_MAX_S * this.sr;
    out[0] = this.l.tapHermite(Math.max(this.centre + clamp(ex * wl + fl * this.fl, -max, max), 1));
    out[1] = this.r.tapHermite(Math.max(this.centre + clamp(ex * wr + fl * this.fr, -max, max), 1));
  }
}

export const glide = (cur, t, c) => { const n = cur + (t - cur) * c; return n === cur ? t : n; };

// reverb::Reverb — 8-line FDN (Householder feedback), two diffusers per side, slewed pre-delay
const RV_N = 8, RV_LINE_MS = [29.7, 37.1, 41.1, 43.7, 53.3, 59.9, 67.7, 79.1], RV_MAX_PRE = 0.25, RV_MAX_SIZE = 2.0, RV_MAX_DECAY = 20;
export const defaultReverb = () => ({ decay: 3.0, size: 1.0, dampHz: 6000, predelayMs: 20 });
export class Reverb {
  constructor(sr) {
    this.sr = sr; const n = nextPow2(Math.floor(RV_LINE_MS[7] * 0.001 * sr * RV_MAX_SIZE * 1.15) + 16);
    this.lines = RV_LINE_MS.map((ms, i) => ({ buf: new Float32Array(n), mask: n - 1, pos: 0, len: ms * 0.001 * sr, lp: 0, lfo: i * 0.125, lfoInc: (0.11 + 0.043 * i) / sr }));
    const sc = sr / 44100, ap = l => new Allpass(Math.floor(l * sc), 0.6);
    this.diff = [[ap(142), ap(107)], [ap(151), ap(113)]];
    this.pre = [new DelayLine(Math.floor(RV_MAX_PRE * sr) + 4), new DelayLine(Math.floor(RV_MAX_PRE * sr) + 4)];
    this.targetLen = new Float32Array(RV_N); this.gain = new Float32Array(RV_N); this.out = new Float32Array(RV_N);
    this.set(defaultReverb()); this.snap();
  }
  set(p) { const size = clamp(p.size, 0.5, RV_MAX_SIZE), decay = clamp(p.decay, 0.1, RV_MAX_DECAY);
    for (let i = 0; i < RV_N; i++) { const len = RV_LINE_MS[i] * 0.001 * this.sr * size; this.targetLen[i] = len; this.gain[i] = Math.pow(10, -3 * (len / this.sr) / decay); }
    this.damp = Math.exp(-TAU * clamp(p.dampHz, 500, 20000) / this.sr); this.preTarget = Math.max(clamp(p.predelayMs, 0, RV_MAX_PRE * 1000) * 0.001 * this.sr, 0); }
  snap() { for (let i = 0; i < RV_N; i++) this.lines[i].len = this.targetLen[i]; this.preLen = this.preTarget; }
  reset() { for (const l of this.lines) { l.buf.fill(0); l.lp = 0; } for (const s of this.diff) for (const a of s) a.clear(); this.pre[0].clear(); this.pre[1].clear(); }
  tick(inL, inR, o) {
    this.preLen = glide(this.preLen, this.preTarget, 0.0008); this.pre[0].push(inL); this.pre[1].push(inR);
    let dl, dr; if (this.preLen < 1) { dl = inL; dr = inR; } else { dl = this.pre[0].tapLinear(this.preLen); dr = this.pre[1].tapLinear(this.preLen); }
    dl = this.diff[0][1].tick(this.diff[0][0].tick(dl)); dr = this.diff[1][1].tick(this.diff[1][0].tick(dr));
    const out = this.out, mod = 0.00028 * this.sr; let sum = 0;
    for (let i = 0; i < RV_N; i++) { const l = this.lines[i]; l.len = glide(l.len, this.targetLen[i], 0.0004); l.lfo += l.lfoInc; l.lfo -= Math.floor(l.lfo);
      const d = Math.max(l.len + Math.sin(TAU * l.lfo) * mod, 2), di = d | 0, f = d - di;
      const a = l.buf[(l.pos - 1 - di) & l.mask], b = l.buf[(l.pos - 2 - di) & l.mask], x = a + (b - a) * f;
      l.lp = undenorm((1 - this.damp) * x + this.damp * l.lp); out[i] = l.lp * this.gain[i]; sum += out[i]; }
    const k = sum * (2 / RV_N); let wl = 0, wr = 0;
    for (let i = 0; i < RV_N; i++) { const l = this.lines[i], inj = (i & 1) === 0 ? dl : dr, sign = ((i >> 1) & 1) === 0 ? 1 : -1;
      l.buf[l.pos & l.mask] = undenorm(out[i] - k + inj * 0.35); l.pos++; if ((i & 1) === 0) wl += out[i] * sign; else wr += out[i] * sign; }
    o[0] = wl * 0.5; o[1] = wr * 0.5;
  }
}

// limiter::Limiter — true-peak (4x polyphase Kaiser FIR) lookahead limiter, sliding-min + moving average gain
const LT_TAPS = 32, LT_HALF = 16, LT_DET = 17;
function bessel0(x) { let s = 1, t = 1; for (let k = 1; k < 30; k++) { const h = x / (2 * k); t *= h * h; s += t; } return s; }
const LT_TP = (() => { const out = [], beta = 9, i0 = bessel0(beta);
  for (let k = 0; k < 3; k++) { const f = (k + 1) / 4, row = new Float32Array(LT_TAPS);
    for (let m = 0; m < LT_TAPS; m++) { const d = m - (LT_HALF - 1) - f, sinc = Math.abs(d) < 1e-9 ? 1 : Math.sin(PI * d) / (PI * d), r = d / LT_HALF;
      row[m] = sinc * (Math.abs(r) >= 1 ? 0 : bessel0(beta * Math.sqrt(1 - r * r)) / i0); } out.push(row); } return out; })();
export class Limiter {
  constructor(sr, lookMs, relMs) {
    const l = Math.max(Math.round(lookMs / 1000 * sr), 8); this.l = l; this.w = 2 * l - 1; this.d = l + LT_DET - 1; const cap = this.w + 2;
    this.ceil = 1; this.rel = 1 - Math.exp(-1 / (relMs / 1000 * sr));
    this.hist = [new Float32Array(LT_TAPS * 2), new Float32Array(LT_TAPS * 2)]; this.hpos = 0;
    this.delay = [new Float32Array(this.d), new Float32Array(this.d)]; this.dqVal = new Float32Array(cap); this.dqIdx = new Float64Array(cap); this.avg = new Float32Array(l);
    this.reset();
  }
  latency() { return this.d; }
  setCeilingDb(db) { this.ceil = Math.pow(10, (db - 0.3) * 0.05); }
  setReleaseMs(sr, ms) { this.rel = 1 - Math.exp(-1 / (Math.max(ms, 0.1) / 1000 * sr)); }
  setOff() { this.ceil = 1e4; }
  reset() { this.hist[0].fill(0); this.hist[1].fill(0); this.hpos = 0; this.delay[0].fill(0); this.delay[1].fill(0); this.dpos = 0; this.dqHead = 0; this.dqLen = 0;
    this.avg.fill(1); this.avgPos = 0; this.avgSum = this.l; this.n = 0; this.gr = 1; }
  tick(l, r, o) {
    let peak = 0; const th = this.ceil * 0.3, hp = this.hpos = (this.hpos + 1) % LT_TAPS;
    for (let c = 0; c < 2; c++) { const h = this.hist[c], x = c === 0 ? l : r; h[hp + LT_TAPS - 1] = x; if (hp > 0) h[hp - 1] = x; else h[LT_TAPS * 2 - 1] = x;
      // window is h[hp .. hp+31] (oldest .. newest)
      const a = Math.abs(h[hp + LT_HALF - 1]); if (a > peak) peak = a;
      let loud = 0; for (let j = LT_HALF - 4; j < LT_HALF + 4; j++) { const v = Math.abs(h[hp + j]); if (v > loud) loud = v; }
      if (loud > th) for (let k = 0; k < 3; k++) { const t = LT_TP[k]; let y = 0; for (let m = 0; m < LT_TAPS; m++) y += t[m] * h[hp + m]; if (Math.abs(y) > peak) peak = Math.abs(y); } }
    const t = peak > this.ceil ? this.ceil / peak : 1, cap = this.dqVal.length, s = this.n++;
    while (this.dqLen > 0) { const last = (this.dqHead + this.dqLen - 1) % cap; if (this.dqVal[last] >= t) this.dqLen--; else break; }
    const tail = (this.dqHead + this.dqLen) % cap; this.dqVal[tail] = t; this.dqIdx[tail] = s; this.dqLen++;
    while (this.dqIdx[this.dqHead] + this.w <= s) { this.dqHead = (this.dqHead + 1) % cap; this.dqLen--; }
    const m = this.dqVal[this.dqHead]; this.avgSum += m - this.avg[this.avgPos]; this.avg[this.avgPos] = m; this.avgPos = (this.avgPos + 1) % this.l;
    let g = Math.min(this.avgSum / this.l, 1); const rec = this.gr + (1 - this.gr) * this.rel; this.gr = g < rec ? g : rec; if (this.gr > 0.9999) this.gr = 1;
    const dp = this.dpos, xl = this.delay[0][dp], xr = this.delay[1][dp]; this.delay[0][dp] = l; this.delay[1][dp] = r; this.dpos = dp + 1 === this.d ? 0 : dp + 1;
    o[0] = xl * this.gr; o[1] = xr * this.gr; return this.gr;
  }
}

// eq — RBJ designs with Q on shelves, and EqBand with a coefficient ramp (no zipper on moves)
export const Shape = { Peak: 0, LowShelf: 1, HighShelf: 2, HighPass: 3, LowPass: 4, Notch: 5, BandPass: 6 };
const om = (fc, sr) => { const w = TAU * clamp(fc, 5, sr * 0.495) / sr; return [Math.sin(w), Math.cos(w)]; };
const nrm = Coeffs.norm;
export const eq = {
  peak(fc, q, g, sr) { const a = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(1 + al * a, -2 * c, 1 - al * a, 1 + al / a, -2 * c, 1 - al / a); },
  lowShelf(fc, q, g, sr) { const a = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)), sq = 2 * Math.sqrt(a) * al;
    return nrm(a * ((a + 1) - (a - 1) * c + sq), 2 * a * ((a - 1) - (a + 1) * c), a * ((a + 1) - (a - 1) * c - sq), (a + 1) + (a - 1) * c + sq, -2 * ((a - 1) + (a + 1) * c), (a + 1) + (a - 1) * c - sq); },
  highShelf(fc, q, g, sr) { const a = Math.pow(10, g / 40), [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)), sq = 2 * Math.sqrt(a) * al;
    return nrm(a * ((a + 1) + (a - 1) * c + sq), -2 * a * ((a - 1) + (a + 1) * c), a * ((a + 1) + (a - 1) * c - sq), (a + 1) - (a - 1) * c + sq, 2 * ((a - 1) - (a + 1) * c), (a + 1) - (a - 1) * c - sq); },
  highpass(fc, q, sr) { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm((1 + c) / 2, -(1 + c), (1 + c) / 2, 1 + al, -2 * c, 1 - al); },
  lowpass(fc, q, sr) { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm((1 - c) / 2, 1 - c, (1 - c) / 2, 1 + al, -2 * c, 1 - al); },
  notch(fc, q, sr) { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(1, -2 * c, 1, 1 + al, -2 * c, 1 - al); },
  bandpass(fc, q, sr) { const [s, c] = om(fc, sr), al = s / (2 * Math.max(q, 0.05)); return nrm(al, 0, -al, 1 + al, -2 * c, 1 - al); },
  design(shape, fc, q, g, sr) {
    switch (shape) {
      case 0: return Math.abs(g) < 1e-3 ? Coeffs.IDENTITY : this.peak(fc, q, g, sr);
      case 1: return Math.abs(g) < 1e-3 ? Coeffs.IDENTITY : this.lowShelf(fc, q, g, sr);
      case 2: return Math.abs(g) < 1e-3 ? Coeffs.IDENTITY : this.highShelf(fc, q, g, sr);
      case 3: return this.highpass(fc, q, sr); case 4: return this.lowpass(fc, q, sr); case 5: return this.notch(fc, q, sr); default: return this.bandpass(fc, q, sr);
    }
  },
};
const lerpC = (a, b, t) => ({ b0: a.b0 + (b.b0 - a.b0) * t, b1: a.b1 + (b.b1 - a.b1) * t, b2: a.b2 + (b.b2 - a.b2) * t, a1: a.a1 + (b.a1 - a.a1) * t, a2: a.a2 + (b.a2 - a.a2) * t });
export class EqBand {
  constructor(ramp) { this.st = [new Biquad(), new Biquad()]; this.cur = this.from = this.to = Coeffs.IDENTITY; this.ramp = Math.max(ramp, 1); this.pos = 0; this.key = null; }
  set(shape, fc, q, g, sr) { const k = shape + ',' + fc + ',' + q + ',' + g; if (k === this.key) return; const first = this.key === null; this.key = k; this.to = eq.design(shape, fc, q, g, sr);
    if (first) { this.cur = this.from = this.to; this.pos = this.ramp; } else { this.from = this.cur; this.pos = 0; } }
  reset() { this.cur = this.from = this.to; this.pos = this.ramp; this.st[0].reset(); this.st[1].reset(); }
  isIdentity() { const t = this.to; return this.pos >= this.ramp && t.b0 === 1 && t.b1 === 0 && t.b2 === 0 && t.a1 === 0 && t.a2 === 0; }
  advance() { if (this.pos < this.ramp) { this.pos++; if (this.pos % 16 === 0 || this.pos === this.ramp) this.cur = lerpC(this.from, this.to, this.pos / this.ramp); } }
  tick(l, r, o) { this.advance(); const c = this.cur; o[0] = this.st[0].tick(l, c); o[1] = this.st[1].tick(r, c); }
  tickCh(ch, x) { return this.st[ch & 1].tick(x, this.cur); }
}

// sat — 2x oversampled waveshaper (Kaiser half-band), Tape / Tube / Diode curves, DC blocker
const OS_TAPS = 49;
const OS_H = (() => { const i0 = x => { let s = 1, t = 1; for (let k = 1; k < 40; k++) { const h = x / (2 * k); t *= h * h; s += t; } return s; };
  const beta = 8, mid = (OS_TAPS - 1) / 2, h = new Float32Array(OS_TAPS); let sum = 0;
  for (let i = 0; i < OS_TAPS; i++) { const d = i - mid, sinc = d === 0 ? 1 : Math.sin(PI * d * 0.5) / (PI * d * 0.5), r = d / mid; const v = 0.5 * sinc * i0(beta * Math.sqrt(Math.max(1 - r * r, 0))) / i0(beta); h[i] = v; sum += v; }
  for (let i = 0; i < OS_TAPS; i++) h[i] /= sum; return h; })();
export class Oversampler2x {
  static LATENCY = (OS_TAPS - 1) / 2;
  constructor() { this.up_ = new Float32Array(OS_TAPS); this.dn = new Float32Array(OS_TAPS); this.upPos = 0; this.dnPos = 0; this.o = [0, 0]; }
  reset() { this.up_.fill(0); this.dn.fill(0); }
  up(x) { this.upPos = (this.upPos + OS_TAPS - 1) % OS_TAPS; this.up_[this.upPos] = x; let ye = 0, yo = 0;
    for (let k = 0; k < 25; k++) { const v = this.up_[(this.upPos + k) % OS_TAPS]; ye += OS_H[2 * k] * v; if (2 * k + 1 < OS_TAPS) yo += OS_H[2 * k + 1] * v; }
    this.o[0] = 2 * ye; this.o[1] = 2 * yo; return this.o; }
  down(s0, s1) { this.dnPos = (this.dnPos + OS_TAPS - 1) % OS_TAPS; this.dn[this.dnPos] = s0; let acc = 0;
    for (let k = 0; k < OS_TAPS; k++) acc += OS_H[k] * this.dn[(this.dnPos + k) % OS_TAPS];
    this.dnPos = (this.dnPos + OS_TAPS - 1) % OS_TAPS; this.dn[this.dnPos] = s1; return acc; }
}
export const SatCurve = {
  shape(c, x) {
    if (c === 0) { x = clamp(x, -4.5, 4.5); const x2 = x * x; return x * (27 + x2) / (27 + 9 * x2); }
    if (c === 1) { const y = x >= 0 ? 1 - Math.exp(-x * 1.15) : -(1 - Math.exp(x * 0.85)) / 0.85 * 0.85; return clamp(y, -1.25, 1.0); }
    const a = Math.abs(x), y = a < 0.6 ? a : 0.6 + 0.4 * (1 - Math.exp(-(a - 0.6) * 2.5)); return x < 0 ? -y : y;
  },
  makeup(c, drive) { const x = 0.25; return x / Math.max(Math.abs(this.shape(c, x * drive)), 1e-6); },
};
export class Saturator {
  constructor() { const lat = Oversampler2x.LATENCY; this.os = [new Oversampler2x(), new Oversampler2x()]; this.dry = [new DelayLine(lat + 4), new DelayLine(lat + 4)]; this.dc = [0, 0]; this.dcx = [0, 0]; }
  latency() { return Oversampler2x.LATENCY; }
  reset() { this.os[0].reset(); this.os[1].reset(); this.dry[0].clear(); this.dry[1].clear(); this.dc = [0, 0]; this.dcx = [0, 0]; }
  tick(l, r, curve, drive, makeup, mix, o) {
    for (let ch = 0; ch < 2; ch++) { const x = ch === 0 ? l : r; this.dry[ch].push(x); const d = this.dry[ch].tapInt(Oversampler2x.LATENCY);
      const u = this.os[ch].up(x * drive), y = this.os[ch].down(SatCurve.shape(curve, u[0]), SatCurve.shape(curve, u[1])) * makeup;
      const hp = y - this.dcx[ch] + 0.999 * this.dc[ch]; this.dcx[ch] = y; this.dc[ch] = undenorm(hp); o[ch] = d + (hp - d) * mix; }
  }
}

// modfx — Flanger, FeedbackDelay (tone filters in the loop, ping-pong), width, balance, Correlation
export class Flanger {
  static MAX_SECONDS = 0.012;
  constructor(sr) { const n = Math.floor(Flanger.MAX_SECONDS * sr) + 8; this.sr = sr; this.lines = [new DelayLine(n), new DelayLine(n)]; this.phase = 0; this.fb = [0, 0]; }
  reset() { this.lines[0].clear(); this.lines[1].clear(); this.phase = 0; this.fb = [0, 0]; }
  tick(l, r, rate, depth, feedback, o) {
    this.phase += rate / this.sr; if (this.phase >= 1) this.phase -= 1;
    const base = 0.0006 * this.sr, span = 0.0085 * this.sr * clamp(depth, 0, 1), fb = clamp(feedback, -0.95, 0.95);
    for (let ch = 0; ch < 2; ch++) { const x = ch === 0 ? l : r, ph = this.phase + (ch === 1 ? 0.25 : 0), fr = ph - Math.floor(ph);
      const tri = 1 - 2 * Math.abs(fr * 2 - 1), d = base + span * 0.5 * (1 + tri), wet = this.lines[ch].tapHermite(Math.max(d, 1));
      this.lines[ch].push(x + undenorm(this.fb[ch] * fb)); this.fb[ch] = wet; o[ch] = wet; }
  }
}
export class FeedbackDelay {
  static MAX_SECONDS = 2.0;
  constructor(sr) { const n = Math.floor(FeedbackDelay.MAX_SECONDS * sr) + 16; this.sr = sr; this.lines = [new DelayLine(n), new DelayLine(n)];
    this.lp = [new Biquad(), new Biquad()]; this.hp = [new Biquad(), new Biquad()]; this.lpC = Coeffs.IDENTITY; this.hpC = Coeffs.IDENTITY; this.toneKey = ''; this.time = new Smooth(0.25, sr, 120, 120); }
  reset(sec) { this.lines[0].clear(); this.lines[1].clear(); for (const b of [...this.lp, ...this.hp]) b.reset(); this.time.reset(sec); }
  setTone(lpHz, hpHz) { const k = lpHz + ',' + hpHz; if (k === this.toneKey) return; this.toneKey = k; this.lpC = Coeffs.lowpass(Math.min(lpHz, this.sr * 0.45), 0.6, this.sr); this.hpC = Coeffs.highpass(hpHz, 0.6, this.sr); }
  tick(l, r, seconds, feedback, pingPong, o) {
    const t = this.time.tick(clamp(seconds, 0.001, FeedbackDelay.MAX_SECONDS - 0.001)), d = t * this.sr;
    const wl = this.lines[0].tapHermite(d), wr = this.lines[1].tapHermite(d), fb = clamp(feedback, 0, 0.97);
    const fl = this.hp[0].tick(this.lp[0].tick(wl, this.lpC), this.hpC) * fb, fr = this.hp[1].tick(this.lp[1].tick(wr, this.lpC), this.hpC) * fb;
    if (pingPong) { this.lines[0].push(undenorm(0.5 * (l + r) + fr)); this.lines[1].push(undenorm(fl)); } else { this.lines[0].push(undenorm(l + fl)); this.lines[1].push(undenorm(r + fr)); }
    o[0] = wl; o[1] = wr;
  }
}
export const width = (l, r, w, o) => { const m = 0.5 * (l + r), s = 0.5 * (l - r) * w; o[0] = m + s; o[1] = m - s; };
export const balance = (l, r, p, o) => { p = clamp(p, -1, 1); const a = (p + 1) * PI / 4; o[0] = l * Math.min(Math.cos(a) * Math.SQRT2, 1); o[1] = r * Math.min(Math.sin(a) * Math.SQRT2, 1); };
export class Correlation {
  constructor(sr) { this.a = 1 - Math.exp(-1 / (0.3 * sr)); this.lr = 0; this.ll = 0; this.rr = 0; }
  push(l, r) { this.lr = undenorm(this.lr + (l * r - this.lr) * this.a); this.ll = undenorm(this.ll + (l * l - this.ll) * this.a); this.rr = undenorm(this.rr + (r * r - this.rr) * this.a); }
  value() { const d = Math.sqrt(this.ll * this.rr); return d < 1e-9 ? 1 : clamp(this.lr / d, -1, 1); }
}

// dynamics — Envelope, gain computer (soft knee), DynBand: y = x + (G-1)·F(x) with a split-band detector
export class Envelope {
  constructor(sr, a, r) { this.sr = sr; this.env = 0; this.setTimes(a, r); }
  setTimes(a, r) { const c = ms => 1 - Math.exp(-1 / (Math.max(ms, 0.05) * 0.001 * this.sr)); this.att = c(a); this.rel = c(r); }
  reset() { this.env = 0; }
  tick(x) { x = Math.abs(x); const c = x > this.env ? this.att : this.rel; this.env = undenorm(this.env + (x - this.env) * c); return this.env; }
}
export const DynMode = { Off: 0, Compress: 1, Expand: 2 };
export function gainComputer(mode, lvl, th, ratio, knee, range) {
  if (mode === 0) return 0; const over = lvl - th, k = Math.max(knee, 0.01);
  const x = over <= -k * 0.5 ? 0 : over >= k * 0.5 ? over : ((over + k * 0.5) ** 2) / (2 * k);
  const g = x * (1 - 1 / Math.max(ratio, 1)); return mode === 1 ? -Math.min(g, Math.abs(range)) : Math.min(g, Math.abs(range));
}
export const BandKind = { Bell: 0, LowShelf: 1, HighShelf: 2,
  coeffs(kind, fc, q, sr) { return kind === 0 ? eq.bandpass(fc, q, sr) : kind === 1 ? eq.lowpass(fc, 0.6, sr) : eq.highpass(fc, 0.6, sr); } };
export class DynBand {
  constructor(sr) { this.sel = [new Biquad(), new Biquad()]; this.det = [new Biquad(), new Biquad()]; this.c = Coeffs.IDENTITY; this.key = ''; this.env = new Envelope(sr, 5, 120);
    this.gainDb = 0; this.smooth = 0; this.sr = sr; this.staticDb = null; this.o = [0, 0]; }
  reset() { for (const b of [...this.sel, ...this.det]) b.reset(); this.env.reset(); this.gainDb = 0; }
  prepare(s) { const k = s.kind + ',' + s.freq + ',' + s.q; if (k !== this.key) { this.key = k; this.c = BandKind.coeffs(s.kind, s.freq, s.q, this.sr); }
    this.env.setTimes(s.attack_ms, s.release_ms); this.smooth = 1 - Math.exp(-1 / (0.002 * this.sr)); if (this.staticDb === null) this.staticDb = s.gain_db; }
  tick(l, r, s, o) {
    const fl = this.sel[0].tick(l, this.c), fr = this.sel[1].tick(r, this.c); let target = 0;
    if (s.mode !== 0) { const dl = this.det[0].tick(l, this.c), dr = this.det[1].tick(r, this.c), e = this.env.tick(Math.max(Math.abs(dl), Math.abs(dr)));
      target = gainComputer(s.mode, 20 * Math.log10(Math.max(e, 1e-6)), s.threshold_db, s.ratio, 6, s.range_db); }
    this.gainDb = undenorm(this.gainDb + (target - this.gainDb) * this.smooth);
    // Browser demo: the static gain is glided with the same 2 ms smoother so knob moves never click.
    this.staticDb += (s.gain_db - this.staticDb) * this.smooth;
    const total = this.staticDb + this.gainDb; if (Math.abs(total) < 1e-4) { o[0] = l; o[1] = r; return; }
    const k = dbToGain(total) - 1; o[0] = l + k * fl; o[1] = r + k * fr;
  }
}

// timewarp::TimeWarp — tempo-synced warp inside each period: Tape (varispeed pointer) or Stretch (WSOLA grains)
export class TimeWarp {
  static MAX_PERIOD_SECONDS = 4.0;
  constructor(sr) {
    this.g = Math.max(Math.floor(Math.floor(0.043 * sr) / 2) * 2, 64); this.maxPeriod = Math.floor(TimeWarp.MAX_PERIOD_SECONDS * sr);
    const size = nextPow2(2 * this.maxPeriod + 4 * this.g + 64); this.buf = [new Float32Array(size), new Float32Array(size)]; this.mask = size - 1;
    this.xfLen = Math.max(Math.floor(0.006 * sr), 8); this.win = new Float32Array(this.g); for (let i = 0; i < this.g; i++) this.win[i] = 0.5 - 0.5 * Math.cos(TAU * i / this.g);
    this.o = [0, 0]; this.reset();
  }
  latency() { return this.g; }
  reset() { this.buf[0].fill(0); this.buf[1].fill(0); this.w = 2 * this.maxPeriod + 2 * this.g; this.base = this.now(); this.acc = 0; this.old = this.base; this.xf = 0;
    this.grains = [{ start: 0, t: 0, on: false }, { start: 0, t: 0, on: false }]; this.hopPos = 0; }
  now() { return this.w - 1 - this.g; }
  at(ch, i) { return this.buf[ch][i & this.mask]; }
  read(ch, pos) { const i = Math.floor(pos), f = pos - i; if (f === 0) return this.at(ch, i); const b = this.buf[ch], m = this.mask;
    const ym1 = b[(i - 1) & m], y0 = b[i & m], y1 = b[(i + 1) & m], y2 = b[(i + 2) & m];
    const c1 = 0.5 * (y1 - ym1), c2 = ym1 - 2.5 * y0 + 2 * y1 - 0.5 * y2, c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1); return ((c3 * f + c2) * f + c1) * f + y0; }
  align(nominal, cont) {
    if (Math.abs(cont - nominal) < 0.5) return nominal;
    const latest = this.w - 1 - this.g, search = this.g >> 2, len = this.g >> 2, c0 = Math.round(cont), n0 = Math.round(nominal), b0 = this.buf[0], b1 = this.buf[1], m = this.mask;
    const mono = i => b0[i & m] + b1[i & m];
    const score = st => { let dot = 0, en = 1e-9; for (let k = 0; k < len; k += 2) { const b = mono(st + k); dot += mono(c0 + k) * b; en += b * b; } return dot / Math.sqrt(en); };
    let best = 0, bs = score(n0);
    for (let d = -search; d <= search; d++) if (d !== 0 && n0 + d <= latest) { const sc = score(n0 + d); if (sc > bs) { bs = sc; best = d; } }
    return nominal + best;
  }
  tick(l, r, rate, stretch, newPeriod, period, o) {
    const wi = this.w & this.mask; this.buf[0][wi] = Number.isFinite(l) ? l : 0; this.buf[1][wi] = Number.isFinite(r) ? r : 0; this.w++;
    rate = clamp(rate, 0.5, 2.0); period = clamp(period, 16, this.maxPeriod); const now = this.now();
    if (newPeriod) { const prev = this.base + this.acc; this.base = rate <= 1 ? now : now - period; this.acc = 0; if (Math.abs(prev - this.base) > 0.5) { this.old = prev; this.xf = 1; } }
    const oldest = this.w - this.mask + 8, pos = Math.max(Math.min(this.base + this.acc, now), oldest); this.acc += rate;
    if (!stretch) {
      let a = this.read(0, pos), b = this.read(1, pos);
      if (this.xf > 0) { const g = 0.5 + 0.5 * Math.cos(PI * this.xf / this.xfLen), op = Math.min(this.old, now); a += (this.read(0, op) - a) * g; b += (this.read(1, op) - b) * g;
        this.old += rate; this.xf++; if (this.xf > this.xfLen) this.xf = 0; }
      o[0] = a; o[1] = b; return;
    }
    if (this.hopPos === 0) {
      const g0 = this.grains[0], slot = (g0.on && g0.t < this.g / 2) ? 1 : (!g0.on || g0.t >= this.g) ? 0 : 1, other = this.grains[1 - slot];
      const start = other.on ? this.align(pos, other.start + other.t) : pos; this.grains[slot] = { start, t: 0, on: true };
    }
    this.hopPos = (this.hopPos + 1) % (this.g / 2);
    let a = 0, b = 0;
    for (let k = 0; k < 2; k++) { const gr = this.grains[k]; if (!gr.on) continue; const p = Math.min(gr.start + gr.t, this.w - 1), wv = this.win[gr.t];
      a += wv * this.read(0, p); b += wv * this.read(1, p); gr.t++; if (gr.t >= this.g) gr.on = false; }
    o[0] = a; o[1] = b;
  }
}

// fft::Fft (radix-2) and stft::Stft — Hann analysis/synthesis at 75 % overlap, latency = size
export class Fft {
  constructor(n) { this.n = n; const bits = Math.log2(n); this.rev = new Uint32Array(n);
    for (let i = 0; i < n; i++) { let r = 0, x = i; for (let b = 0; b < bits; b++) { r = (r << 1) | (x & 1); x >>= 1; } this.rev[i] = r; }
    this.cos = new Float32Array(n / 2); this.sin = new Float32Array(n / 2); for (let k = 0; k < n / 2; k++) { this.cos[k] = Math.cos(TAU * k / n); this.sin[k] = -Math.sin(TAU * k / n); } }
  forward(re, im) { const n = this.n;
    for (let i = 0; i < n; i++) { const j = this.rev[i]; if (j > i) { let t = re[i]; re[i] = re[j]; re[j] = t; t = im[i]; im[i] = im[j]; im[j] = t; } }
    for (let len = 2; len <= n; len <<= 1) { const half = len >> 1, step = n / len;
      for (let st = 0; st < n; st += len) for (let k = 0; k < half; k++) { const wr = this.cos[k * step], wi = this.sin[k * step], a = st + k, b = a + half;
        const tr = re[b] * wr - im[b] * wi, ti = re[b] * wi + im[b] * wr; re[b] = re[a] - tr; im[b] = im[a] - ti; re[a] += tr; im[a] += ti; } } }
}
export const wrapPhase = p => { const t = p + PI; return t - Math.floor(t / TAU) * TAU - PI; };
export class Stft {
  constructor(size) { this.n = size; this.hop = size / 4; this.fft = new Fft(size); this.win = new Float32Array(size); for (let i = 0; i < size; i++) this.win[i] = 0.5 - 0.5 * Math.cos(TAU * i / size);
    this.scale = 1 / (1.5 * size); const bins = size / 2 + 1; this.ch = [0, 1].map(() => ({ inp: new Float32Array(size), out: new Float32Array(size) })); this.pos = 0;
    this.re = new Float32Array(size); this.im = new Float32Array(size); this.mag = [new Float32Array(bins), new Float32Array(bins)]; this.phase = [new Float32Array(bins), new Float32Array(bins)]; }
  bins() { return this.n / 2 + 1; }
  latency() { return this.n; }
  reset() { for (const c of this.ch) { c.inp.fill(0); c.out.fill(0); } for (const m of [...this.mag, ...this.phase]) m.fill(0); this.pos = 0; }
  tick(l, r, p, o) { const i = this.pos, c0 = this.ch[0], c1 = this.ch[1]; o[0] = c0.out[i]; o[1] = c1.out[i]; c0.out[i] = 0; c1.out[i] = 0; c0.inp[i] = l; c1.inp[i] = r;
    this.pos = (i + 1) % this.n; if (this.pos % this.hop === 0) { if (p.beginFrame) p.beginFrame(); this.frame(0, p); this.frame(1, p); } }
  frame(c, p) {
    const n = this.n, st = this.pos, re = this.re, im = this.im, inp = this.ch[c].inp, w = this.win, bins = n / 2 + 1, mag = this.mag[c], ph = this.phase[c];
    for (let k = 0; k < n; k++) { re[k] = inp[(st + k) % n] * w[k]; im[k] = 0; }
    this.fft.forward(re, im);
    for (let k = 0; k < bins; k++) { const a = re[k], b = im[k]; mag[k] = Math.sqrt(a * a + b * b); ph[k] = Math.atan2(b, a); }
    p.process(c, mag, ph);
    for (let k = 0; k < bins; k++) { const m = Number.isFinite(mag[k]) ? mag[k] : 0; re[k] = m * Math.cos(ph[k]); im[k] = -(m * Math.sin(ph[k])); }
    im[0] = 0; im[n / 2] = 0; for (let k = bins; k < n; k++) { re[k] = re[n - k]; im[k] = -im[n - k]; }
    this.fft.forward(re, im); const out = this.ch[c].out, sc = this.scale;
    for (let k = 0; k < n; k++) out[(st + k) % n] += undenorm(re[k] * w[k] * sc);
  }
}
