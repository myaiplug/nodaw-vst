// NoDAW browser demo — page side. The DSP runs in ./worklet.js (the plugin's engine ported to an AudioWorklet).
import { CHOP_PATTERNS } from './dsp.js';
const root = document.getElementById('demo');
const PLUG = root.dataset.plugin, PROC = root.dataset.proc;
const WORKLET = new URL('./worklet.js', location.href).href, META = new URL('./meta.json', location.href).href;
const LOOP = new URL('../assets/demo-loop.mp3', location.href).href, LOOP_BPM = 90, LOOP_SEC = 16 * 60 / LOOP_BPM;
const $ = s => root.querySelector(s);
const css = getComputedStyle(document.documentElement);
const ACC = (getComputedStyle(root).getPropertyValue('--acc') || '#a78bfa').trim(), ACC2 = (getComputedStyle(root).getPropertyValue('--acc2') || '#e879f9').trim();
const st = { meta: null, params: {}, defs: {}, ui: {}, ctx: null, buf: null, isLoop: true, name: 'Demo loop · 90 BPM (synthesised)', bpm: LOOP_BPM,
  playing: false, node: null, src: null, out: null, t0: 0, off: 0, info: null, peaks: null, hold: [-90, -90], workletReady: null };

function err(msg) { const e = $('.err'); e.textContent = msg; e.style.display = 'block'; }
// ---------- parameter mapping (apf curves) ----------
function toNorm(p, v) { const lo = p.min, hi = p.max; v = Math.min(hi, Math.max(lo, v));
  if (p.curve === 'Curve::Log' && lo > 0) return Math.log(v / lo) / Math.log(hi / lo);
  const m = /Power\(([\d.]+)\)/.exec(p.curve || ''); const t = (v - lo) / (hi - lo); return m ? Math.pow(t, 1 / parseFloat(m[1])) : t; }
function fromNorm(p, t) { const lo = p.min, hi = p.max; t = Math.min(1, Math.max(0, t));
  if (p.curve === 'Curve::Log' && lo > 0) return lo * Math.pow(hi / lo, t);
  const m = /Power\(([\d.]+)\)/.exec(p.curve || ''); return lo + (hi - lo) * (m ? Math.pow(t, parseFloat(m[1])) : t); }
function fmt(p, v) { const d = p.decimals != null ? parseInt(p.decimals, 10) : (Math.abs(p.max - p.min) > 50 ? 0 : 1);
  let s = Number(v).toFixed(isNaN(d) ? 1 : d); if (p.unit === 'Hz' && v >= 1000) s = (v / 1000).toFixed(v >= 10000 ? 1 : 2) + 'k';
  return s + (p.unit ? (p.unit === 'Hz' && v >= 1000 ? 'Hz' : ' ' + p.unit) : ''); }

// ---------- audio ----------
function send(msg) { if (st.node) st.node.port.postMessage(msg); }
function set(id, v, quiet) { st.params[id] = v; send({ params: { [id]: v } }); if (!quiet && st.ui[id]) st.ui[id](v); }
async function ensureCtx() {
  if (!st.ctx) { const AC = window.AudioContext || window.webkitAudioContext; st.ctx = new AC({ latencyHint: 'interactive' });
    if (!st.ctx.audioWorklet) throw new Error('This browser has no AudioWorklet support.');
    st.workletReady = st.ctx.audioWorklet.addModule(WORKLET);
    const c = st.ctx; st.out = c.createGain(); st.out.gain.value = 0; st.anIn = c.createAnalyser(); st.anOut = c.createAnalyser();
    for (const a of [st.anIn, st.anOut]) { a.fftSize = 4096; a.smoothingTimeConstant = 0.78; }
    st.split = c.createChannelSplitter(2); st.anL = c.createAnalyser(); st.anR = c.createAnalyser(); st.anL.fftSize = st.anR.fftSize = 2048;
    st.out.connect(st.anOut); st.out.connect(c.destination); st.out.connect(st.split); st.split.connect(st.anL, 0); st.split.connect(st.anR, 1); }
  await st.workletReady; if (st.ctx.state !== 'running') await st.ctx.resume();
}
function makeNode(c, params, startFrame) {
  return new AudioWorkletNode(c, PROC, { numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2], channelCount: 2, channelCountMode: 'explicit', channelInterpretation: 'speakers',
    processorOptions: { params: { ...params }, tempo: st.bpm, startFrame } });
}
function startAt(offset) {
  const c = st.ctx, t0 = c.currentTime + 0.06, dur = st.isLoop ? LOOP_SEC : st.buf.duration; offset = ((offset % dur) + dur) % dur;
  const src = c.createBufferSource(); src.buffer = st.buf; src.loop = true; if (st.isLoop) { src.loopStart = 0; src.loopEnd = Math.min(LOOP_SEC, st.buf.duration); }
  const node = makeNode(c, st.params, Math.round((t0 - offset) * c.sampleRate));
  node.port.onmessage = e => { if (e.data && e.data.info) st.info = e.data.info; };
  src.connect(node); node.connect(st.out); src.connect(st.anIn); src.start(t0, offset);
  st.out.gain.cancelScheduledValues(t0); st.out.gain.setValueAtTime(0, t0); st.out.gain.linearRampToValueAtTime(1, t0 + 0.03);
  st.src = src; st.node = node; st.t0 = t0 - offset; st.playing = true; $('.play').classList.add('on'); $('.play').textContent = '■ Stop';
}
function stopNow(cb) {
  if (!st.playing) { cb && cb(); return; }
  const c = st.ctx, t = c.currentTime, src = st.src, node = st.node; st.playing = false; st.node = null; st.src = null;
  st.out.gain.cancelScheduledValues(t); st.out.gain.setValueAtTime(st.out.gain.value, t); st.out.gain.linearRampToValueAtTime(0, t + 0.035);
  setTimeout(() => { try { src.stop(); } catch (e) {} src.disconnect(); node.port.postMessage({ stop: true }); node.disconnect(); cb && cb(); }, 60);
  $('.play').classList.remove('on'); $('.play').textContent = '▶ Play';
}
async function toggle() {
  try { await ensureCtx(); } catch (e) { err('Audio could not start: ' + e.message); return; }
  if (st.playing) stopNow(); else startAt(0);
}
function position() { if (!st.playing) return 0; const dur = st.isLoop ? LOOP_SEC : st.buf.duration; return ((st.ctx.currentTime - st.t0) % dur + dur) % dur; }

// ---------- sources ----------
async function decode(ab) { const oc = new OfflineAudioContext(2, 1, 44100); return await oc.decodeAudioData(ab); }
async function loadDemo() { const r = await fetch(LOOP); if (!r.ok) throw new Error('demo loop ' + r.status); st.buf = await decode(await r.arrayBuffer()); st.isLoop = true; buildPeaks(); }
async function loadFile(f) {
  if (!f) return; $('.srcname').textContent = 'Decoding ' + f.name + '…';
  try { const b = await decode(await f.arrayBuffer()); const wasPlaying = st.playing;
    stopNow(() => { st.buf = b; st.isLoop = false; st.name = f.name; $('.srcname').textContent = f.name; buildPeaks(); if (wasPlaying) startAt(0); });
    if (!wasPlaying) { st.buf = b; st.isLoop = false; st.name = f.name; $('.srcname').textContent = f.name; buildPeaks(); }
    $('.bpmhint').style.display = 'block';
  } catch (e) { $('.srcname').textContent = st.name; err('Could not decode that file (' + (e.message || e) + '). Try WAV, MP3, M4A or OGG.'); }
}
function useDemo() { const go = () => loadDemo().then(() => { st.name = 'Demo loop · 90 BPM (synthesised)'; $('.srcname').textContent = st.name; setBpm(LOOP_BPM); $('.bpmhint').style.display = 'none'; });
  if (st.playing) stopNow(() => go().then(() => startAt(0))); else go(); }
function setBpm(v) { v = Math.min(240, Math.max(40, +v || 120)); st.bpm = v; $('#bpm').value = v; send({ tempo: v }); }

// ---------- offline render (export + self-test) ----------
async function render(params, seconds) {
  const b = st.buf, sr = b.sampleRate, oc = new OfflineAudioContext(2, Math.ceil(seconds * sr), sr);
  await oc.audioWorklet.addModule(WORKLET);
  const p = { ...params }; if (PLUG === 'chronos-weaver' && p.source === 'Snapshot') p.capture = true; // a fresh engine captures B at the start
  const s = oc.createBufferSource(); s.buffer = b; if (st.isLoop) { s.loop = true; s.loopEnd = Math.min(LOOP_SEC, b.duration); }
  const n = makeNode(oc, p, 0); s.connect(n); n.connect(oc.destination); s.start(0); return await oc.startRendering();
}
function wav(buf) {
  const n = buf.length, ch = 2, out = new DataView(new ArrayBuffer(44 + n * ch * 2)), w = (o, s) => { for (let i = 0; i < s.length; i++) out.setUint8(o + i, s.charCodeAt(i)); };
  w(0, 'RIFF'); out.setUint32(4, 36 + n * ch * 2, true); w(8, 'WAVEfmt '); out.setUint32(16, 16, true); out.setUint16(20, 1, true); out.setUint16(22, ch, true);
  out.setUint32(24, buf.sampleRate, true); out.setUint32(28, buf.sampleRate * ch * 2, true); out.setUint16(32, ch * 2, true); out.setUint16(34, 16, true); w(36, 'data'); out.setUint32(40, n * ch * 2, true);
  const L = buf.getChannelData(0), R = buf.getChannelData(1); let o = 44;
  for (let i = 0; i < n; i++) { for (const x of [L[i], R[i]]) { const v = Math.max(-1, Math.min(1, x)); out.setInt16(o, v < 0 ? v * 32768 : v * 32767, true); o += 2; } }
  return new Blob([out], { type: 'audio/wav' });
}
async function exportWav() {
  const btn = $('.exp'); btn.disabled = true; const label = btn.textContent; btn.textContent = 'Rendering…';
  try { const secs = st.isLoop ? 2 * LOOP_SEC : Math.min(st.buf.duration + 3, 240); const r = await render(st.params, secs);
    const a = document.createElement('a'); a.href = URL.createObjectURL(wav(r)); const pr = $('#preset').value || 'custom';
    a.download = `nodaw-${PLUG}-${pr.replace(/[^a-z0-9]+/gi, '-').toLowerCase()}${st.params.bypass ? '-bypass' : ''}.wav`; document.body.appendChild(a); a.click(); setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 2000);
  } catch (e) { err('Export failed: ' + e.message); } finally { btn.disabled = false; btn.textContent = label; }
}
// self-test hook: processed vs bypass on the demo loop, second pass (after warm-up)
async function selfTest(presetName) {
  if (!st.isLoop) await loadDemo();
  const p = { ...st.defs, ...((st.meta.presets[presetName] || {}).params || {}) };
  const a = await render({ ...p, bypass: false }, 2 * LOOP_SEC), b = await render({ ...p, bypass: true }, 2 * LOOP_SEC);
  const from = Math.floor(LOOP_SEC * a.sampleRate); let d = 0, e = 0, be = 0, nan = 0;
  for (let c = 0; c < 2; c++) { const x = a.getChannelData(c), y = b.getChannelData(c); for (let i = from; i < x.length; i++) { if (!Number.isFinite(x[i])) nan++; const q = x[i] - y[i]; d += q * q; e += x[i] * x[i]; be += y[i] * y[i]; } }
  const n = 2 * (a.length - from), db = v => 10 * Math.log10(v / n + 1e-24);
  return { preset: presetName, outDb: db(e), bypassDb: db(be), diffDb: db(d), nonFinite: nan, sampleRate: a.sampleRate };
}

// ---------- UI ----------
function buildControls() {
  const ps = st.meta.params.filter(p => p.kind !== 'bypass'), mods = [];
  for (const p of ps) { const m = p.module || 'Main'; let g = mods.find(x => x.name === m); if (!g) mods.push(g = { name: m, ps: [] }); g.ps.push(p); }
  const host = $('.ctls'), tabbed = ps.length > 30;
  const modEl = g => { const d = document.createElement('div'); d.className = 'mod'; d.innerHTML = `<h4><span>${g.name}</span><span class="mono" style="color:var(--mute)">${g.ps.length}</span></h4>`;
    for (const p of g.ps) d.appendChild(control(p)); return d; };
  if (!tabbed) { const w = document.createElement('div'); w.className = 'mods'; mods.forEach(g => w.appendChild(modEl(g))); host.appendChild(w); return; }
  host.classList.add('tabbed'); const tabs = document.createElement('div'); tabs.className = 'tabs'; const grid = document.createElement('div'); grid.className = 'mods';
  const show = i => { [...tabs.children].forEach((b, k) => b.classList.toggle('sel', k === i)); grid.innerHTML = ''; grid.appendChild(modEl(mods[i])); };
  mods.forEach((g, i) => { const b = document.createElement('button'); b.type = 'button'; b.textContent = g.name; b.onclick = () => show(i); tabs.appendChild(b); });
  host.appendChild(tabs); host.appendChild(grid); show(0);
}
function control(p) {
  const row = document.createElement('div'); row.className = 'ctl'; const lab = document.createElement('label'); lab.textContent = p.name; lab.title = p.name; row.appendChild(lab);
  if (p.kind === 'float') {
    const r = document.createElement('input'); r.type = 'range'; r.min = 0; r.max = 1000; r.step = 1; r.setAttribute('aria-label', p.name); const o = document.createElement('output');
    const ui = v => { const t = toNorm(p, v); r.value = Math.round(t * 1000); r.style.setProperty('--p', (t * 100).toFixed(1) + '%'); o.textContent = fmt(p, v); };
    r.oninput = () => { let v = fromNorm(p, r.value / 1000); const d = parseInt(p.decimals, 10); if (!isNaN(d)) v = +v.toFixed(Math.max(d, p.max - p.min <= 2 ? 3 : d)); set(p.id, v, true); ui(v); markCustom(); };
    r.ondblclick = () => { set(p.id, p.def); markCustom(); }; row.appendChild(r); row.appendChild(o); st.ui[p.id] = ui;
  } else if (p.kind === 'choice') {
    const short = p.choices.length <= 5 && p.choices.every(c => c.length <= 9);
    if (short) { const seg = document.createElement('div'); seg.className = 'seg';
      p.choices.forEach(c => { const b = document.createElement('button'); b.type = 'button'; b.textContent = c; b.onclick = () => { set(p.id, c); markCustom(); }; seg.appendChild(b); });
      st.ui[p.id] = v => [...seg.children].forEach(b => b.classList.toggle('sel', b.textContent === v)); row.appendChild(seg);
    } else { const s = document.createElement('select'); s.setAttribute('aria-label', p.name); p.choices.forEach(c => s.add(new Option(c, c))); s.onchange = () => { set(p.id, s.value, true); markCustom(); };
      st.ui[p.id] = v => { s.value = v; }; row.appendChild(s); }
  } else if (p.kind === 'bool') {
    if (PLUG === 'chronos-weaver' && p.id === 'capture') { const b = document.createElement('button'); b.type = 'button'; b.className = 'btn mom'; b.textContent = '◉ Capture B';
      b.onclick = () => { set('capture', true); setTimeout(() => set('capture', false), 400); }; st.ui[p.id] = v => b.classList.toggle('primary', !!v); row.appendChild(b);
    } else { const b = document.createElement('button'); b.type = 'button'; b.className = 'sw'; b.setAttribute('role', 'switch'); b.setAttribute('aria-label', p.name);
      b.onclick = () => { set(p.id, !st.params[p.id]); markCustom(); }; st.ui[p.id] = v => { b.classList.toggle('on', !!v); b.setAttribute('aria-checked', !!v); }; row.appendChild(b); }
  }
  return row;
}
// Screw: the chop gate's 16 steps, drawn like the plugin (click cycles full → half → off; editing switches to Custom)
let stepEls = null;
function buildSteps() {
  const host = $('.steps'); if (!host || PLUG !== 'screw') return; host.style.display = 'grid'; stepEls = [];
  if (!Array.isArray(st.params.custom_pattern)) st.params.custom_pattern = CHOP_PATTERNS[5][1].slice();
  for (let i = 0; i < 16; i++) { const b = document.createElement('button'); b.type = 'button'; b.setAttribute('aria-label', 'Chop step ' + (i + 1));
    b.onclick = () => { const cur = patternNow().slice(); cur[i] = cur[i] === 2 ? 1 : cur[i] === 1 ? 0 : 2; st.params.custom_pattern = cur; send({ params: { custom_pattern: cur } }); set('chop_pattern', 'Custom'); markCustom(); };
    host.appendChild(b); stepEls.push(b); }
}
function patternNow() { const n = st.params.chop_pattern; const f = CHOP_PATTERNS.find(p => p[0] === n); return f ? f[1] : st.params.custom_pattern; }
function drawSteps() { if (!stepEls) return; const pat = patternNow(), on = st.playing && st.info && !st.params.bypass ? st.info.step : -1;
  stepEls.forEach((b, i) => { b.dataset.l = pat[i]; b.classList.toggle('now', i === on); }); }
function markCustom() { const s = $('#preset'); if (s.value && !s.value.endsWith('*')) { /* keep name, flag edits */ $('.pdesc').dataset.edited = '1'; $('.pdesc').textContent = (st.meta.presets[s.value] || {}).description + ' · edited'; } }
function applyPreset(name) {
  const pr = st.meta.presets[name]; if (!pr) return; const keep = st.params.bypass;
  const next = { ...st.defs, ...pr.params, bypass: keep, capture: false }; Object.assign(st.params, next); send({ params: next });
  for (const id in st.ui) st.ui[id](st.params[id]); $('.pdesc').textContent = pr.description || ''; $('.pdesc').dataset.edited = '';
}
function setAB(byp) { set('bypass', byp); $('.ab .a').classList.toggle('sel', !byp); $('.ab .b').classList.toggle('sel', byp); }

// ---------- drawing ----------
function fitCanvas(cv) { const r = cv.getBoundingClientRect(), d = Math.min(window.devicePixelRatio || 1, 2); const w = Math.max(1, Math.round(r.width * d)), h = Math.max(1, Math.round(r.height * d));
  if (cv.width !== w || cv.height !== h) { cv.width = w; cv.height = h; } return d; }
function buildPeaks() { const b = st.buf, n = 1200, L = b.getChannelData(0), R = b.numberOfChannels > 1 ? b.getChannelData(1) : L, len = st.isLoop ? Math.min(b.length, Math.round(LOOP_SEC * b.sampleRate)) : b.length, pk = new Float32Array(n);
  for (let i = 0; i < n; i++) { const a = Math.floor(i * len / n), z = Math.floor((i + 1) * len / n); let m = 0; for (let k = a; k < z; k += 4) m = Math.max(m, Math.abs(L[k]), Math.abs(R[k])); pk[i] = m; } st.peaks = pk; }
const FMIN = 20, FMAX = 20000, DBMIN = -96, DBMAX = 0;
function drawSpectrum(cv) {
  const d = fitCanvas(cv), g = cv.getContext('2d'), W = cv.width, H = cv.height; g.clearRect(0, 0, W, H);
  const fx = f => Math.log(f / FMIN) / Math.log(FMAX / FMIN) * W, dy = db => (1 - (db - DBMIN) / (DBMAX - DBMIN)) * H;
  g.strokeStyle = 'rgba(154,145,176,.12)'; g.lineWidth = 1; g.fillStyle = 'rgba(154,145,176,.55)'; g.font = `${10 * d}px JetBrains Mono, monospace`;
  for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) { const x = fx(f); g.beginPath(); g.moveTo(x, 0); g.lineTo(x, H); g.stroke(); g.fillText(f >= 1000 ? f / 1000 + 'k' : f, x + 3 * d, H - 6 * d); }
  for (let db = -84; db < 0; db += 12) { const y = dy(db); g.beginPath(); g.moveTo(0, y); g.lineTo(W, y); g.stroke(); }
  if (!st.ctx) return; const sr = st.ctx.sampleRate;
  const curve = an => { const a = new Float32Array(an.frequencyBinCount); an.getFloatFrequencyData(a); const pts = []; const cols = Math.min(W, 600);
    for (let i = 0; i <= cols; i++) { const f0 = FMIN * Math.pow(FMAX / FMIN, i / cols), f1 = FMIN * Math.pow(FMAX / FMIN, (i + 1) / cols); let k0 = Math.floor(f0 / sr * an.fftSize), k1 = Math.max(k0, Math.floor(f1 / sr * an.fftSize)); let m = -200;
      for (let k = k0; k <= k1 && k < a.length; k++) m = Math.max(m, a[k]); pts.push([i / cols * W, dy(Math.max(DBMIN, m + 6))]); } return pts; };
  if (st.playing) {
    const pin = curve(st.anIn); g.beginPath(); pin.forEach(([x, y], i) => i ? g.lineTo(x, y) : g.moveTo(x, y)); g.strokeStyle = 'rgba(220,214,236,.35)'; g.lineWidth = 1.2 * d; g.stroke();
    const po = curve(st.anOut); const grd = g.createLinearGradient(0, 0, 0, H); grd.addColorStop(0, ACC + 'cc'); grd.addColorStop(1, ACC2 + '08');
    g.beginPath(); g.moveTo(0, H); po.forEach(([x, y]) => g.lineTo(x, y)); g.lineTo(W, H); g.closePath(); g.fillStyle = grd; g.globalAlpha = .45; g.fill(); g.globalAlpha = 1;
    g.beginPath(); po.forEach(([x, y], i) => i ? g.lineTo(x, y) : g.moveTo(x, y)); g.strokeStyle = ACC; g.lineWidth = 1.8 * d; g.shadowColor = ACC; g.shadowBlur = 10 * d; g.stroke(); g.shadowBlur = 0;
  }
  const I = st.info; if (PLUG === 'chronos-weaver' && I && I.bdb && st.playing) { g.beginPath(); I.bdb.forEach((v, j) => { const f = 20 * Math.pow(1000, (j + 0.5) / 32), x = fx(f), y = dy(Math.max(DBMIN, v)); j ? g.lineTo(x, y) : g.moveTo(x, y); });
    g.setLineDash([5 * d, 4 * d]); g.strokeStyle = ACC2; g.lineWidth = 1.4 * d; g.stroke(); g.setLineDash([]); }
}
function drawWave(cv) {
  const d = fitCanvas(cv), g = cv.getContext('2d'), W = cv.width, H = cv.height; g.clearRect(0, 0, W, H); if (!st.peaks) return;
  const pk = st.peaks, n = pk.length, pos = st.buf ? position() / (st.isLoop ? LOOP_SEC : st.buf.duration) : 0;
  for (let x = 0; x < W; x += 2 * d) { const v = pk[Math.min(n - 1, Math.floor(x / W * n))], h = Math.max(1, v * H * 0.9); g.fillStyle = x / W < pos && st.playing ? ACC : 'rgba(154,145,176,.38)'; g.fillRect(x, (H - h) / 2, 1.2 * d, h); }
  if (st.playing) { g.fillStyle = '#fff'; g.fillRect(pos * W, 0, 1.5 * d, H); }
  if (st.isLoop) { g.fillStyle = 'rgba(154,145,176,.5)'; g.font = `${9 * d}px JetBrains Mono, monospace`; for (let b = 1; b < 4; b++) g.fillRect(b / 4 * W, 0, 1, 5 * d); }
}
const bars = {};
function drawMeter() {
  const el = $('.meter'); if (!st.playing || !st.anL) { for (const k of ['l', 'r']) { el.querySelector('.' + k + ' i').style.width = '0%'; } return; }
  const t = new Float32Array(st.anL.fftSize); const lvl = [];
  for (const [k, an] of [['l', st.anL], ['r', st.anR]]) { an.getFloatTimeDomainData(t); let pk = 0, s = 0; for (const v of t) { pk = Math.max(pk, Math.abs(v)); s += v * v; } lvl.push([20 * Math.log10(pk + 1e-9), 10 * Math.log10(s / t.length + 1e-12)]); }
  lvl.forEach(([pk, rms], i) => { const k = i ? 'r' : 'l', pc = v => Math.max(0, Math.min(100, (v + 60) / 60 * 100)); st.hold[i] = Math.max(pk, st.hold[i] - 0.4);
    el.querySelector('.' + k + ' i').style.width = pc(rms + 3).toFixed(1) + '%'; el.querySelector('.' + k + ' b').style.left = pc(st.hold[i]).toFixed(1) + '%'; });
  el.querySelector('.mval').textContent = `peak ${Math.max(...st.hold).toFixed(1)} dBFS`;
}
function readout() {
  const I = st.info, o = $('.readout'); if (!I || !st.playing) { o.textContent = st.params.bypass ? 'BYPASS' : ''; return; }
  const db = g => (20 * Math.log10(Math.max(g, 1e-6))).toFixed(1); let s = '';
  if (PLUG === 'screw') s = `LIMIT GR ${db(I.gr)} dB\nCHOP STEP ${String(I.step + 1).padStart(2, '0')}/16`;
  else if (PLUG === 'racks') s = `REACTOR GR ${db(I.gr)} dB\nCORRELATION ${I.corr.toFixed(2)}`;
  else if (PLUG === 'harmonic-space') s = `CORRELATION ${I.corr.toFixed(2)}`;
  else if (PLUG === 'chronos-dynamic-eq') s = 'DYN dB  ' + I.dyn.map(v => (v >= 0 ? '+' : '') + v.toFixed(1)).join(' ');
  else if (PLUG === 'temporal-pitch-portal') s = `PITCH ${I.pitch >= 0 ? '+' : ''}${I.pitch.toFixed(2)} st\nWARP PERIOD ${'▮'.repeat(Math.round(I.phase * 8))}${'▯'.repeat(8 - Math.round(I.phase * 8))}`;
  else if (PLUG === 'chronos-weaver') s = `MORPH ${(I.morph * 100).toFixed(0)}%  FREEZE ${(I.freeze * 100).toFixed(0)}%\nB ${st.params.source === 'Snapshot' ? (I.hasSnapshot ? 'SNAPSHOT HELD' : 'NO SNAPSHOT · PRESS CAPTURE B') : 'PAST ' + Math.round(st.params.weave) + ' ms'}`;
  o.textContent = (st.params.bypass ? 'BYPASS (A/B)\n' : '') + s;
}
function frame() { drawSpectrum($('#spec')); drawWave($('.wave')); drawMeter(); readout(); drawSteps(); requestAnimationFrame(frame); }

// ---------- boot ----------
async function boot() {
  const r = await fetch(META); st.meta = await r.json();
  for (const p of st.meta.params) st.defs[p.id] = p.kind === 'bypass' ? false : p.kind === 'choice' ? p.choices[p.def] : p.def;
  Object.assign(st.params, st.defs); buildControls(); buildSteps(); for (const id in st.ui) st.ui[id](st.params[id]);
  const sel = $('#preset'); sel.add(new Option('— Factory presets —', ''));
  const names = Object.keys(st.meta.presets).sort((a, b) => (a === 'Init') - (b === 'Init') || a.localeCompare(b)); for (const n of names) sel.add(new Option(n, n));
  sel.onchange = () => sel.value && applyPreset(sel.value);
  const first = root.dataset.preset; if (first && st.meta.presets[first]) { sel.value = first; applyPreset(first); }
  $('.play').onclick = toggle; $('.ab .a').onclick = () => setAB(false); $('.ab .b').onclick = () => setAB(true);
  $('.exp').onclick = exportWav; $('.load').onclick = () => $('#file').click(); $('#file').onchange = e => loadFile(e.target.files[0]); $('.usedemo').onclick = useDemo;
  $('#bpm').onchange = e => setBpm(e.target.value);
  $('.wave').onclick = e => { if (!st.playing) return; const r = e.currentTarget.getBoundingClientRect(), dur = st.isLoop ? LOOP_SEC : st.buf.duration; const off = (e.clientX - r.left) / r.width * dur; stopNow(() => startAt(off)); };
  const con = root; ['dragenter', 'dragover'].forEach(t => con.addEventListener(t, e => { e.preventDefault(); con.classList.add('drag'); }));
  ['dragleave', 'drop'].forEach(t => con.addEventListener(t, e => { e.preventDefault(); if (t === 'drop' || e.target === con) con.classList.remove('drag'); }));
  con.addEventListener('drop', e => { const f = e.dataTransfer && e.dataTransfer.files[0]; if (f) loadFile(f); });
  document.addEventListener('keydown', e => { if (e.code === 'Space' && !/INPUT|SELECT|TEXTAREA|BUTTON/.test(document.activeElement.tagName)) { e.preventDefault(); toggle(); } });
  await loadDemo(); $('.srcname').textContent = st.name; requestAnimationFrame(frame);
}
window.NoDAWDemo = { ready: boot().then(() => true).catch(e => { err('Demo failed to load: ' + e.message); throw e; }), selfTest, state: st,
  async workletLoads() { const oc = new OfflineAudioContext(2, 128, 44100); await oc.audioWorklet.addModule(WORKLET); new AudioWorkletNode(oc, PROC, { outputChannelCount: [2] }); return true; } };
