#!/usr/bin/env python3
"""A stand-in for apf-webview for the NoDAW plugin suite (every plugin built on ui-kit/), so a
plugin's real page (ui/index.html, style.css, app.js, ui-kit/kit.js|css and the real apf.js)
can be rendered and driven in a normal browser where there is no WebView2.

The parameter table comes from the plugin itself (`cargo run -p <crate> --example ui_fixture`),
the factory presets from <plugin>/presets/*.apfpreset. Meter and spectrum frames are synthetic:
a looping 120 bpm programme, with each plugin's response modelled roughly (EQ curves, pitch
shift) so the displays move the way they do in a host. Plugin endpoints the mock does not know
store what the page POSTs and give it back on GET.

    python3 suite_server.py --plugin racks [--port 8777]
"""
import argparse, json, math, os, struct, subprocess, threading, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
KIT = os.path.join(ROOT, "ui-kit")
APFJS = os.path.join(ROOT, "vendor", "apf", "crates", "apf-webview", "src", "apf.js")
BANDS, HEAD, FLO, FHI = 160, 16, 20.0, 20000.0
IPC_SHIM = "<script>window.ipc={postMessage:m=>fetch('/__mock/ipc',{method:'POST',body:String(m),keepalive:true})};</script>"


def biquad_db(shape, fc, q, g, f, sr=48000.0):
    """|H| in dB of the RBJ forms in nodaw-dsp/src/eq.rs."""
    w = 2 * math.pi * min(max(fc, 1), sr * 0.49) / sr
    s, c = math.sin(w), math.cos(w)
    al = s / (2 * max(q, 0.05))
    A = 10 ** (g / 40)
    if shape in ("peak", "lowShelf", "highShelf") and abs(g) < 1e-3: return 0.0
    if shape == "peak": b = (1 + al * A, -2 * c, 1 - al * A); a = (1 + al / A, -2 * c, 1 - al / A)
    elif shape == "lowShelf":
        sq = 2 * math.sqrt(A) * al
        b = (A * (A + 1 - (A - 1) * c + sq), 2 * A * (A - 1 - (A + 1) * c), A * (A + 1 - (A - 1) * c - sq)); a = (A + 1 + (A - 1) * c + sq, -2 * (A - 1 + (A + 1) * c), A + 1 + (A - 1) * c - sq)
    elif shape == "highShelf":
        sq = 2 * math.sqrt(A) * al
        b = (A * (A + 1 + (A - 1) * c + sq), -2 * A * (A - 1 + (A + 1) * c), A * (A + 1 + (A - 1) * c - sq)); a = (A + 1 - (A - 1) * c + sq, 2 * (A - 1 - (A + 1) * c), A + 1 - (A - 1) * c - sq)
    elif shape == "highpass": b = ((1 + c) / 2, -(1 + c), (1 + c) / 2); a = (1 + al, -2 * c, 1 - al)
    elif shape == "lowpass": b = ((1 - c) / 2, 1 - c, (1 - c) / 2); a = (1 + al, -2 * c, 1 - al)
    elif shape == "bandpass": b = (al, 0.0, -al); a = (1 + al, -2 * c, 1 - al)
    elif shape == "notch": b = (1, -2 * c, 1); a = (1 + al, -2 * c, 1 - al)
    else: return 0.0
    ww = 2 * math.pi * f / sr
    def mag(co):
        re = co[0] + co[1] * math.cos(ww) + co[2] * math.cos(2 * ww); im = -(co[1] * math.sin(ww) + co[2] * math.sin(2 * ww)); return re * re + im * im
    return 10 * math.log10(max(1e-12, mag(b) / max(1e-12, mag(a))))


class Model:
    def __init__(self, plugin, fixture):
        self.plugin = plugin
        self.pdir = os.path.join(ROOT, "plugins", f"nodaw-{plugin}")
        self.ui = os.path.join(self.pdir, "ui")
        self.defs = fixture["params"]
        self.ranges = fixture["ranges"]
        self.fixture = fixture
        self.index = {d["key"]: i for i, d in enumerate(self.defs)}
        self.vals = [self.default_norm(d) for d in self.defs]
        self.rev, self.gen = 1, 1
        self.store = {}           # plugin endpoints the mock does not model
        self.ab = None
        self.preset, self.changed = None, False
        self.t0 = time.time()
        self.playing, self.level = True, 1.0
        self.lock = threading.Lock()
        self.presets = self.load_presets()
        self.license = self.lic("trial", 11)

    def lic(self, state, days):
        return {"state": state, "days_left": days, "trial_days": 14, "busy": False, "message": None, "ok": False,
                "email": "beez@example.com" if state == "licensed" else None, "key_hint": "••••9F2C" if state == "licensed" else None,
                "buy_url": f"https://nodawlabs.gumroad.com/l/nodaw-{self.plugin}", "activation_ready": True}

    # value maps (mirror of apf::param)
    def steps(self, d):
        k = d["kind"]
        return 1 if k == "bool" else (d["max"] - d["min"]) if k == "int" else (len(d["options"]) - 1) if k == "choice" else 0
    def default_norm(self, d):
        k = d["kind"]
        if k == "float": return self.to_norm(d, d["default"])
        if k == "bool": return float(d["default"])
        if k == "int": return (d["default"] - d["min"]) / max(1, d["max"] - d["min"])
        return d["default"] / max(1, len(d["options"]) - 1)
    def to_plain(self, d, n):
        n = min(1.0, max(0.0, n)); c = d.get("curve")
        if c == "log": return d["min"] * (d["max"] / d["min"]) ** n
        if c == "pow": return d["min"] + (d["max"] - d["min"]) * n ** d["exp"]
        return d["min"] + (d["max"] - d["min"]) * n
    def to_norm(self, d, p):
        lo, hi = d["min"], d["max"]; p = min(max(p, min(lo, hi)), max(lo, hi)); c = d.get("curve")
        if c == "log": return math.log(p / lo) / math.log(hi / lo)
        if c == "pow": return ((p - lo) / (hi - lo)) ** (1.0 / d["exp"])
        return (p - lo) / (hi - lo) if hi != lo else 0
    def text(self, d, n):
        k = d["kind"]
        if k == "bool": return "On" if n >= 0.5 else "Off"
        if k == "choice": return d["options"][int(round(n * (len(d["options"]) - 1)))]
        if k == "int": return str(int(round(d["min"] + n * (d["max"] - d["min"]))))
        p = self.to_plain(d, n); dec = d["decimals"]
        if d["unit"] == "Hz" and abs(p) >= 1000 - 0.5 * 10 ** (-dec): return f"{p/1000:.2f} kHz"
        s = f"{p:.{dec}f}"
        return f"{s} {d['unit']}" if d["unit"] else s
    def has(self, key): return key in self.index
    def plain(self, key, dflt=0.0):
        if key not in self.index: return dflt
        d = self.defs[self.index[key]]; n = self.vals[self.index[key]]
        if d["kind"] == "float": return self.to_plain(d, n)
        if d["kind"] == "bool": return n >= 0.5
        return int(round(n * self.steps(d)))
    def set_plain(self, key, v):
        i = self.index[key]; d = self.defs[i]
        if d["kind"] == "float": self.vals[i] = self.to_norm(d, float(v))
        elif d["kind"] == "bool": self.vals[i] = 1.0 if v in (True, 1, 1.0, "1", "On", "true") else 0.0
        elif d["kind"] == "choice":
            opts = d["options"]
            idx = opts.index(v) if isinstance(v, str) and v in opts else int(float(v))
            self.vals[i] = idx / max(1, len(opts) - 1)

    def load_presets(self):
        out = []
        pd = os.path.join(self.pdir, "presets")
        for fn in sorted(os.listdir(pd)):
            if not fn.endswith(".apfpreset"): continue
            j = json.load(open(os.path.join(pd, fn), encoding="utf-8"))
            out.append({"id": "factory:" + j["name"], "name": j["name"], "factory": True, "type": j.get("type", ""), "subtype": j.get("subtype", ""),
                        "mood": ",".join(j.get("mood", [])), "description": j.get("description", ""), "author": j.get("author", ""), "params": j["params"]})
        return out
    def load_preset(self, pid):
        for p in self.presets:
            if p["id"] == pid:
                self.vals = [self.default_norm(d) for d in self.defs]
                for key, v in p["params"].items():
                    if key in self.index: self.set_plain(key, v)
                self.preset, self.changed = p, False; self.rev += 1
                return {"ok": True}
        return {"ok": False, "error": "no such preset"}

    def info(self):
        ps = []
        for i, d in enumerate(self.defs):
            o = {"index": i, "key": d["key"], "name": d["name"], "unit": d["unit"], "kind": d["kind"], "steps": self.steps(d),
                 "default": self.default_norm(d), "value": self.vals[i], "text": self.text(d, self.vals[i])}
            if d["kind"] == "choice": o["options"] = d["options"]
            ps.append(o)
        return {"gen": self.gen, "resize": {"min": [760, 480], "max": [2600, 1700]}, "rev": self.rev, "params": ps,
                "preset": self.preset_json(), "learn": -1, "cc": [], "controllers": {"wheel": 0, "pedal": False, "bend": 0}}
    def preset_json(self):
        p = self.preset
        return None if not p else {"id": p["id"], "name": p["name"], "factory": True, "changed": self.changed}
    def values(self):
        return {"gen": self.gen, "rev": self.rev, "values": [[v, self.text(d, v)] for d, v in zip(self.defs, self.vals)],
                "preset": self.preset_json(), "learn": -1, "cc": [], "controllers": {"wheel": 0, "pedal": False, "bend": 0}}

    # ---- synthetic programme ----
    def music(self, f, i, t, beat, kick):
        base = -26 - 17 * math.log10(max(f, 20) / 60.0) ** 1.35 if f > 60 else -26 + 12 * math.log10(max(f, 20) / 60.0)
        bump = 9 * math.exp(-((math.log2(f / 90.0)) ** 2) / 0.35) * (0.4 + 0.6 * kick)
        bump += 5 * math.exp(-((math.log2(f / 1800.0)) ** 2) / 1.4) * (0.5 + 0.5 * math.sin(0.3 + beat * 0.9))
        jitter = 3.2 * math.sin(f * 0.013 + t * 7.0) * math.sin(f * 0.0041 - t * 3.1) + 1.7 * math.sin(i * 1.7 + t * 11)
        return base + bump + jitter

    def response(self, f):
        """The plugin's static response in dB at f (rough model for the mock)."""
        P = self.plain; r = 0.0
        if self.plugin == "racks" and P("eq_on"):
            for n in range(1, 8):
                if not P(f"eq{n}_on"): continue
                shape = "peak"
                if n == 1: shape = ["lowShelf", "highpass", "peak"][P("eq1_type")]
                if n == 7: shape = ["highShelf", "lowpass", "peak"][P("eq7_type")]
                r += biquad_db(shape, P(f"eq{n}_freq"), P(f"eq{n}_q"), P(f"eq{n}_gain"), f)
        hook = getattr(self, f"resp_{self.plugin.replace('-', '_')}", None)
        if hook: r += hook(f)
        return r

    def frame(self):
        t = time.time() - self.t0
        beat = t * 2.0
        kick = math.exp(-((beat % 1.0) * 5.0))
        env = (0.55 + 0.45 * kick) * self.level
        envdb = 20 * math.log10(max(env, 1e-3))
        ratio = self.pitch_ratio() if hasattr(self, "pitch_ratio") else 1.0
        in_db, out_db = [], []
        mix = self.plain("mix", 100.0) / 100.0
        for i in range(BANDS):
            f = FLO * (FHI / FLO) ** ((i + 0.5) / BANDS)
            a = self.music(f, i, t, beat, kick) + envdb
            fs = f / ratio
            b = (self.music(max(fs, 20), i, t, beat, kick) if fs >= 20 else -90) + envdb + self.response(f)
            b = 10 * math.log10((1 - mix) * 10 ** (a / 10) + mix * 10 ** (b / 10) + 1e-12)
            in_db.append(max(-120, a)); out_db.append(max(-120, b))
        g = 10 ** (self.plain("in_gain", 0.0) / 20); og = 10 ** (self.plain("out_gain", 0.0) / 20)
        pk_in = min(1.6, (0.38 + 0.36 * kick) * self.level * g)
        pk_out = min(1.6, pk_in * og * 1.05)
        gr = 1.0
        ceil = 10 ** (self.plain("lim_ceiling", self.plain("ceiling", 0.0)) / 20)
        push = 10 ** (self.plain("lim_gain", 0.0) / 20)
        if self.plain("lim_on", True) and pk_out * push > ceil: gr = ceil / (pk_out * push); pk_out = ceil
        if hasattr(self, "gr_now"): gr = self.gr_now(t)
        if self.plain("bypass", False): pk_out, gr, out_db = pk_in, 1.0, in_db
        rin, rout = pk_in * 0.42, pk_out * 0.40
        custom = self.custom(t, beat) if hasattr(self, "custom") else [0, 0, 0, 0]
        head = [pk_in, pk_in * 0.93, pk_out, pk_out * 0.96, rin, rin * 0.94, rout, rout * 0.97, gr, custom[0], 48000.0, float(self.fixture.get("latency", 0)),
                1.0 if self.playing else 0.0, custom[1], custom[2], custom[3]]
        extra = self.extras(t, beat, kick) if hasattr(self, "extras") else []
        return struct.pack(f"<{HEAD + 2 * BANDS + len(extra)}f", *head, *in_db, *out_db, *extra)

    def ipc(self, msg):
        parts = msg.split()
        if not parts: return
        with self.lock:
            if parts[0] == "set" and len(parts) == 3:
                i, v = int(parts[1]), float(parts[2])
                if 0 <= i < len(self.vals):
                    self.vals[i] = min(1.0, max(0.0, v))
                    if self.preset: self.changed = True
            elif parts[0] == "log":
                print("[page]", msg[4:])


class Racks(Model):
    def extras(self, t, beat, kick):
        lv = (0.3 + 0.35 * kick) * self.level
        return [lv * 0.9, lv * 1.05, lv * 0.95, min(lv, 10 ** (self.plain("lim_ceiling") / 20)), 0.55 + 0.2 * math.sin(t * 0.7)]


class HarmonicSpace(Model):
    def resp_harmonic_space(self, f):
        P = self.plain
        r = P("amp", 0.0)
        solo = [n for n in range(1, 8) if P(f"b{n}_solo")]
        if solo:
            pw = 0.0
            for n in solo:
                shape = "lowpass" if n == 1 else "highpass" if n == 7 else "bandpass"
                pw += 10 ** (biquad_db(shape, P(f"b{n}_freq"), max(0.5, P(f"b{n}_q")), 0, f) / 10)
            r += 10 * math.log10(pw + 1e-9)
        for n in range(1, 8):
            shape = "lowShelf" if n == 1 else "highShelf" if n == 7 else "peak"
            g = -36.0 if P(f"b{n}_mute") else P(f"b{n}_gain")
            r += biquad_db(shape, P(f"b{n}_freq"), P(f"b{n}_q"), g, f)
        r += biquad_db("lowShelf", P("sub_freq"), 0.8, P("sub"), f)
        if P("hpf") > 20.5: r += biquad_db("highpass", P("hpf"), 0.707, 0, f)
        if P("lpf") < 21900: r += biquad_db("lowpass", P("lpf"), 0.707, 0, f)
        return r + P("out_gain", 0.0)

    def extras(self, t, beat, kick):
        lv = (0.3 + 0.35 * kick) * self.level
        tail = 0.45 + 0.1 * math.sin(t * 1.3)
        return [lv * self.plain("dly_mix") / 100, tail * self.plain("rev_mix") / 100, lv * self.plain("fl_mix") / 100, 0.62 + 0.15 * math.sin(t * 0.5)]


class ChronosDynamicEq(Model):
    def dyn(self, n, t):
        P = self.plain
        mode = P(f"b{n}_mode")
        if not P(f"b{n}_on") or mode == 0: return 0.0
        rg = min(24.0, P(f"b{n}_range") * P("dyn_amount", 100.0) / 100)
        k = math.exp(-(((t * 2.0) + n * 0.13) % 1.0) * 4.0)
        amt = rg * (0.35 + 0.6 * k) * self.level
        return -amt if mode == 1 else amt

    def band_db(self, n, f, g):
        P = self.plain
        kind = P(f"b{n}_type"); fc = P(f"b{n}_freq"); q = P(f"b{n}_q")
        shape = "lowShelf" if kind == 1 else "highShelf" if kind == 2 else "peak"
        return biquad_db(shape, fc, 0.7 if kind else q, g, f)

    def resp_chronos_dynamic_eq(self, f):
        t = time.time() - self.t0
        r = 0.0
        for n in range(1, 8):
            if not self.plain(f"b{n}_on"): continue
            r += self.band_db(n, f, self.plain(f"b{n}_gain") + self.dyn(n, t))
        return r + self.plain("out_gain", 0.0)

    def extras(self, t, beat, kick):
        return [self.dyn(n, t) for n in range(1, 8)]

    def gr_now(self, t):
        cut = min([0.0] + [self.dyn(n, t) for n in range(1, 8)])
        return 10 ** (cut / 20)


PLUGINS = {"racks": Racks, "harmonic-space": HarmonicSpace, "chronos-dynamic-eq": ChronosDynamicEq}


def load_plugin_model(name, fixture):
    cls = PLUGINS.get(name)
    if cls is None:
        import importlib.util
        spec = importlib.util.spec_from_file_location(f"mock_{name}", os.path.join(HERE, f"mock_{name.replace('-', '_')}.py"))
        if spec and os.path.exists(spec.origin):
            mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod); cls = mod.make(Model, biquad_db)
        else:
            cls = Model
    return cls(name, fixture)


def make_handler(model):
    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        def log_message(self, *a): pass
        def send(self, status, body, mime):
            if isinstance(body, str): body = body.encode()
            self.send_response(status); self.send_header("Content-Type", mime); self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store"); self.end_headers(); self.wfile.write(body)
        def js(self, obj, status=200): self.send(status, json.dumps(obj), "application/json")
        def do_POST(self): self.route("POST")
        def do_GET(self): self.route("GET")
        def static(self, base, rel):
            p2 = os.path.normpath(os.path.join(base, rel))
            if p2.startswith(base) and os.path.isfile(p2):
                mime = {".css": "text/css", ".js": "text/javascript", ".woff2": "font/woff2", ".html": "text/html"}.get(os.path.splitext(p2)[1], "application/octet-stream")
                self.send(200, open(p2, "rb").read(), mime); return True
            return False
        def route(self, method):
            u = urllib.parse.urlparse(self.path); q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
            n = int(self.headers.get("Content-Length") or 0); body = self.rfile.read(n) if n else b""
            p, m = u.path, model
            try:
                if p in ("/", "/index.html"):
                    html = open(os.path.join(m.ui, "index.html"), encoding="utf-8").read().replace("<head>", "<head>" + IPC_SHIM, 1)
                    return self.send(200, html, "text/html; charset=utf-8")
                if p == "/favicon.ico": return self.send(204, b"", "text/plain")
                if p == "/__apf/apf.js": return self.send(200, open(APFJS, "rb").read(), "text/javascript")
                if p == "/__mock/ipc": m.ipc(body.decode()); return self.send(204, b"", "text/plain")
                if p == "/__mock/state":
                    with m.lock:
                        for k, v in q.items():
                            if k == "playing": m.playing = v == "1"
                            elif k == "level": m.level = float(v)
                            elif k == "preset": m.load_preset("factory:" + v)
                            elif k == "license": m.license = m.lic(v, int(q.get("days", "11")))
                            elif k == "days": pass
                            elif k == "store":
                                key, _, val = v.partition("=")
                                m.store[key] = val
                            elif k in m.index: m.set_plain(k, v)
                        m.rev += 1
                    return self.js({"ok": True})
                if p == "/__apf/info": return self.js(m.info())
                if p == "/__apf/values": return self.js(m.values())
                if p == "/__apf/presets":
                    return self.js({"presets": [{k: v for k, v in x.items() if k != "params"} for x in m.presets], "dir": f"~/NoDAW/{m.plugin}/Presets"})
                if p == "/__apf/preset/load": return self.js(m.load_preset(q.get("id", "")))
                if p in ("/__apf/learn", "/__apf/forget"): return self.js({"ok": True})
                if p.startswith("/__apf/plugin/"):
                    r = p[len("/__apf/plugin/"):]
                    if r == "frame": return self.send(200, m.frame(), "application/octet-stream")
                    if r == "license": return self.js(m.license)
                    if r == "license-activate":
                        m.license = dict(m.license, busy=False, message="That license does not exist for the provided product.", ok=False); return self.js(m.license)
                    if r == "license-buy": return self.js({"opened": True, "url": m.license["buy_url"]})
                    if r == "ranges": return self.js(m.ranges)
                    if r == "ab":
                        if method == "POST": m.ab = json.loads(body); return self.js(m.ab)
                        return self.js(m.ab)
                    if hasattr(m, "endpoint"):
                        res = m.endpoint(r, method, body, q)
                        if res is not None: return self.js(res)
                    if method == "POST": m.store[r] = body.decode() or "null"; m.rev += 1
                    if r in m.store: return self.send(200, m.store[r], "application/json")
                    if r in m.fixture.get("endpoints", {}): return self.js(m.fixture["endpoints"][r])
                    return self.send(404, "not found", "text/plain")
                rel = p.lstrip("/")
                if self.static(m.ui, rel) or self.static(KIT, rel): return
                self.send(404, "not found", "text/plain")
            except Exception as e:
                import traceback; traceback.print_exc()
                self.send(500, repr(e), "text/plain")
    return H


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--plugin", required=True)
    ap.add_argument("--port", type=int, default=8777)
    ap.add_argument("--fixture", default=None, help="JSON from the plugin's ui_fixture example (default: run it)")
    a = ap.parse_args()
    if a.fixture:
        fx = json.load(open(a.fixture))
    else:
        out = subprocess.check_output(["cargo", "run", "-q", "-p", f"nodaw-{a.plugin}", "--no-default-features", "--example", "ui_fixture"], cwd=ROOT)
        fx = json.loads(out)
    model = load_plugin_model(a.plugin, fx)
    print(f"mock for nodaw-{a.plugin} on http://127.0.0.1:{a.port}/", flush=True)
    ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(model)).serve_forever()
