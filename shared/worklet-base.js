// Shared AudioWorklet wrapper: holds the plugin's parameters (same ids and units as the plugin's
// presets), runs the ported engine on stereo blocks, and gives it a host-like tempo clock.
export function register(name, makeEngine, settingsOf) {
  class NoDawProcessor extends AudioWorkletProcessor {
    constructor(opts) {
      super();
      const o = (opts && opts.processorOptions) || {};
      this.p = Object.assign({}, o.params || {});
      this.tempo = o.tempo || 120;
      this.startFrame = o.startFrame || 0;
      this.engine = makeEngine(sampleRate, this.p);
      this.L = new Float32Array(128); this.R = new Float32Array(128);
      this.info = null; this.n = 0; this.alive = true;
      this.port.onmessage = e => {
        const d = e.data || {};
        if (d.params) Object.assign(this.p, d.params);
        if (d.tempo) this.tempo = d.tempo;
        if (d.cmd && this.engine.command) this.engine.command(d.cmd, d);
        if (d.stop) this.alive = false;
      };
    }
    process(inputs, outputs) {
      const out = outputs[0], inp = inputs[0], n = out[0].length;
      if (this.L.length !== n) { this.L = new Float32Array(n); this.R = new Float32Array(n); }
      const a = inp && inp[0], b = inp && (inp[1] || inp[0]);
      for (let i = 0; i < n; i++) { this.L[i] = a ? a[i] : 0; this.R[i] = b ? b[i] : 0; }
      const beats = (currentFrame - this.startFrame) * this.tempo / (60 * sampleRate);
      const info = this.engine.process(this.L, this.R, settingsOf(this.p), { beats, tempo: this.tempo, playing: true });
      out[0].set(this.L); if (out[1]) out[1].set(this.R);
      if (info && (++this.n & 3) === 0) this.port.postMessage({ info });
      return this.alive;
    }
  }
  registerProcessor(name, NoDawProcessor);
}
// Helpers for settings(): choices are stored by name, like the plugin's presets.
export const idx = (list, v, def = 0) => { if (typeof v === 'number') return v; const i = list.indexOf(v); return i < 0 ? def : i; };
