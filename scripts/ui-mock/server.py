#!/usr/bin/env python3
"""A stand-in for the plugin's web view, so the real UI (ui/index.html, style.css, app.js and
the real apf.js) can be rendered and driven in a normal browser on a machine without WebView2.

It implements the endpoints of apf-webview that the page uses (info, values, presets,
preset/load, plugin/*) and the `window.ipc` bridge, with the parameter table taken from the
plugin itself (`cargo run --example ui_fixture`) and the factory presets from presets/*.apfpreset.
Meter and spectrum frames are synthetic (a looping 140 bpm demo), in the same units as the real
frame: dBFS per band, linear peaks.

    python3 server.py [--port 8765] [--fixture fixture.json]
"""
import argparse, json, math, os, struct, threading, time, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
UI = os.path.join(ROOT, "plugins", "nodaw-screw", "ui")
PRESETS = os.path.join(ROOT, "plugins", "nodaw-screw", "presets")
APFJS = os.path.join(ROOT, "vendor", "apf", "crates", "apf-webview", "src", "apf.js")
BANDS, HEAD, FLO, FHI = 160, 16, 20.0, 20000.0

IPC_SHIM = "<script>window.ipc={postMessage:m=>fetch('/__mock/ipc',{method:'POST',body:String(m),keepalive:true})};</script>"


class Model:
    def __init__(self, fixture):
        self.fx = fixture
        self.defs = fixture["params"]
        self.ranges = fixture["ranges"]
        self.patterns = fixture["patterns"]
        self.lock = threading.Lock()
        self.vals = [self.default_norm(d) for d in self.defs]
        self.rev = 1
        self.gen = 1
        self.custom = [2] * 16
        self.ab = None
        self.preset = None
        self.changed = False
        self.t0 = time.time()
        self.playing = True
        self.level = 1.0          # scenario knob: overall signal level
        self.clip = False
        self.presets = self.load_presets()
        self.index = {d["key"]: i for i, d in enumerate(self.defs)}

    # ---- value maps (mirror of apf::param) ----
    def steps(self, d):
        k = d["kind"]
        if k == "bool": return 1
        if k == "int": return d["max"] - d["min"]
        if k == "choice": return len(d["options"]) - 1
        return 0

    def default_norm(self, d):
        k = d["kind"]
        if k == "float": return self.to_norm(d, d["default"])
        if k == "bool": return float(d["default"])
        if k == "int": return (d["default"] - d["min"]) / max(1, d["max"] - d["min"])
        return d["default"] / max(1, len(d["options"]) - 1)

    def to_plain(self, d, n):
        n = min(1.0, max(0.0, n))
        c = d.get("curve")
        if c == "log": return d["min"] * (d["max"] / d["min"]) ** n
        if c == "pow": return d["min"] + (d["max"] - d["min"]) * n ** d["exp"]
        return d["min"] + (d["max"] - d["min"]) * n

    def to_norm(self, d, p):
        lo, hi = d["min"], d["max"]
        p = min(max(p, min(lo, hi)), max(lo, hi))
        c = d.get("curve")
        if c == "log": return math.log(p / lo) / math.log(hi / lo)
        if c == "pow": return ((p - lo) / (hi - lo)) ** (1.0 / d["exp"])
        return (p - lo) / (hi - lo)

    def text(self, d, n):
        k = d["kind"]
        if k == "bool": return "On" if n >= 0.5 else "Off"
        if k == "choice": return d["options"][int(round(n * (len(d["options"]) - 1)))]
        if k == "int": return str(int(round(d["min"] + n * (d["max"] - d["min"]))))
        p = self.to_plain(d, n)
        dec = d["decimals"]
        if d["unit"] == "Hz" and abs(p) >= 1000 - 0.5 * 10 ** (-dec): return f"{p/1000:.2f} kHz"
        s = f"{p:.{dec}f}"
        return f"{s} {d['unit']}" if d["unit"] else s

    def plain(self, key): return self.to_plain(self.defs[self.index[key]], self.vals[self.index[key]])

    def set_plain(self, key, plain):
        i = self.index[key]; d = self.defs[i]
        if d["kind"] == "float": self.vals[i] = self.to_norm(d, plain)
        elif d["kind"] == "bool": self.vals[i] = float(plain)
        elif d["kind"] == "choice":
            opts = d["options"]; idx = opts.index(plain) if isinstance(plain, str) else int(plain)
            self.vals[i] = idx / (len(opts) - 1)
        self.bump()

    def bump(self):
        self.gen += 0  # gen is the parameter-table generation: unchanged

    def load_presets(self):
        out = []
        for fn in sorted(os.listdir(PRESETS)):
            if not fn.endswith(".apfpreset"): continue
            j = json.load(open(os.path.join(PRESETS, fn), encoding="utf-8"))
            out.append({"id": "factory:" + j["name"], "name": j["name"], "factory": True, "type": j.get("type", ""),
                        "subtype": j.get("subtype", ""), "mood": ",".join(j.get("mood", [])), "description": j.get("description", ""),
                        "author": j.get("author", ""), "params": j["params"], "steps": j.get("steps")})
        return out

    def load_preset(self, pid):
        for p in self.presets:
            if p["id"] == pid:
                for key, v in p["params"].items():
                    if key in self.index: self.set_plain(key, v)
                if p.get("steps"): self.custom = p["steps"]
                self.preset = p; self.changed = False; self.rev += 1
                return {"ok": True}
        return {"ok": False, "error": "no such preset"}

    # ---- JSON the page reads ----
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

    # ---- the synthetic audio-thread frame ----
    def frame(self):
        t = time.time() - self.t0
        bpm = 140.0
        beat = t * bpm / 60.0
        step = int(beat * 4) % 16 if self.playing else 255
        pitch = self.plain("pitch"); ratio = 2 ** (pitch / 12.0)
        lp = self.plain("lowpass"); bass = self.plain("bass")
        width_mix = self.plain("mix") / 100.0
        rev = self.plain("rev_mix") / 100.0
        pat = int(round(self.vals[self.index["chop_pattern"]] * (len(self.defs[self.index["chop_pattern"]]["options"]) - 1)))
        n_pat = len(self.patterns)
        lv = 2
        if self.playing and 0 < pat:
            lv = (self.custom if pat >= n_pat else self.patterns[pat]["steps"])[step]
        depth = self.plain("chop_depth") / 100.0
        gate = {2: 1.0, 1: 1 - 0.5 * depth, 0: 1 - depth}[lv]
        kick = math.exp(-((beat % 1.0) * 5.0))
        env = (0.55 + 0.45 * kick) * self.level
        in_db, out_db = [], []
        for i in range(BANDS):
            f = FLO * (FHI / FLO) ** ((i + 0.5) / BANDS)
            def music(fr, ph):
                base = -26 - 17 * math.log10(max(fr, 20) / 60.0) ** 1.35 if fr > 60 else -26 + 12 * math.log10(max(fr, 20) / 60.0)
                bump = 9 * math.exp(-((math.log2(fr / 90.0)) ** 2) / 0.35) * (0.4 + 0.6 * kick)
                bump += 5 * math.exp(-((math.log2(fr / 1800.0)) ** 2) / 1.4) * (0.5 + 0.5 * math.sin(ph + beat * 0.9))
                jitter = 3.2 * math.sin(fr * 0.013 + t * 7.0 + ph) * math.sin(fr * 0.0041 - t * 3.1) + 1.7 * math.sin(i * 1.7 + t * 11)
                return base + bump + jitter
            a = music(f, 0.3) + 20 * math.log10(max(env, 1e-3))
            fs = f / ratio
            b = (music(max(fs, 20), 0.3) if fs >= 20 else -90) + 20 * math.log10(max(env, 1e-3)) - 1.5
            # tone: lowpass 2nd order, warmth shelf at 150 Hz
            if lp < 19900: b += -10 * math.log10(1 + (f / lp) ** 4)
            if abs(bass) > 0.01: b += bass / (1 + (f / 150.0) ** 2) ** 0.5
            b += 20 * math.log10(max(gate, 1e-3)) if lv != 2 else 0
            # reverb tail fills the valleys
            tail = -48 - 11 * max(0.0, math.log10(max(f, 30) / 100.0)) ** 1.4 + 3 * math.sin(i * .9 + t * 2)
            if rev > 0.01 and f < self.plain("rev_damp") * 1.2: b = 10 * math.log10(10 ** (b / 10) + (rev * 1.3) * 10 ** (tail / 10))
            b = 10 * math.log10((1 - width_mix) * 10 ** (a / 10) + width_mix * 10 ** (b / 10) + 1e-12)
            in_db.append(max(-120, a)); out_db.append(max(-120, b))
        g = 10 ** (self.plain("in_gain") / 20); og = 10 ** (self.plain("out_gain") / 20)
        pk_in = min(1.6, (0.38 + 0.36 * kick) * self.level * g)
        pk_out = min(1.6, pk_in * og * (0.9 if lv == 2 else gate + 0.05))
        gr = 1.0
        if self.vals[self.index["limiter"]] >= 0.5:
            ceil = 10 ** (self.plain("ceiling") / 20)
            if pk_out > ceil: gr = ceil / pk_out; pk_out = ceil
        rms_in = pk_in * 0.42; rms_out = pk_out * 0.40
        head = [pk_in, pk_in * 0.93, pk_out, pk_out * 0.96, rms_in, rms_in * 0.94, rms_out, rms_out * 0.97, gr, float(step), 48000.0, 1536.0,
                1.0 if self.playing else 0.0, 0, 0, 0]
        return struct.pack(f"<{HEAD + 2 * BANDS}f", *head, *in_db, *out_db)

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


def make_handler(model):
    class H(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        def log_message(self, *a): pass
        def send(self, status, body, mime):
            if isinstance(body, str): body = body.encode()
            self.send_response(status)
            self.send_header("Content-Type", mime); self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store"); self.end_headers(); self.wfile.write(body)
        def js(self, obj, status=200): self.send(status, json.dumps(obj), "application/json")
        def do_POST(self): self.route("POST")
        def do_GET(self): self.route("GET")
        def route(self, method):
            u = urllib.parse.urlparse(self.path); q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
            n = int(self.headers.get("Content-Length") or 0); body = self.rfile.read(n) if n else b""
            p = u.path
            m = model
            try:
                if p in ("/", "/index.html"):
                    html = open(os.path.join(UI, "index.html"), encoding="utf-8").read()
                    html = html.replace("<head>", "<head>" + IPC_SHIM, 1) if "<head>" in html else IPC_SHIM + html
                    return self.send(200, html, "text/html; charset=utf-8")
                if p == "/favicon.ico": return self.send(204, b"", "text/plain")
                if p == "/__apf/apf.js": return self.send(200, open(APFJS, "rb").read(), "text/javascript")
                if p == "/__mock/ipc": m.ipc(body.decode()); return self.send(204, b"", "text/plain")
                if p == "/__mock/state":
                    for k, v in q.items():
                        if k == "playing": m.playing = v == "1"
                        elif k == "level": m.level = float(v)
                        elif k == "preset": m.load_preset("factory:" + v)
                        else:
                            try: v = float(v)
                            except ValueError: pass
                            m.set_plain(k, v)
                    return self.js({"ok": True})
                if p == "/__apf/info": return self.js(m.info())
                if p == "/__apf/values": return self.js(m.values())
                if p == "/__apf/presets":
                    return self.js({"presets": [{k: v for k, v in x.items() if k not in ("params", "steps")} for x in m.presets], "dir": "~/NoDAW/Screw/Presets"})
                if p == "/__apf/preset/load": return self.js(m.load_preset(q.get("id", "")))
                if p in ("/__apf/learn", "/__apf/forget"): return self.js({"ok": True})
                if p.startswith("/__apf/plugin/"):
                    r = p[len("/__apf/plugin/"):]
                    if r == "frame": return self.send(200, m.frame(), "application/octet-stream")
                    if r == "ranges": return self.js(m.ranges)
                    if r == "patterns": return self.js(m.patterns)
                    if r == "steps":
                        if method == "POST": m.custom = json.loads(body); m.rev += 1; return self.js({"ok": True})
                        return self.js(m.custom)
                    if r == "ab":
                        if method == "POST": m.ab = json.loads(body); m.rev += 1; return self.js({"ok": True})
                        return self.js(m.ab) if m.ab else self.send(204, b"", "text/plain")
                p2 = os.path.normpath(os.path.join(UI, p.lstrip("/")))
                if p2.startswith(UI) and os.path.isfile(p2):
                    mime = {".css": "text/css", ".js": "text/javascript", ".woff2": "font/woff2", ".html": "text/html"}.get(os.path.splitext(p2)[1], "application/octet-stream")
                    return self.send(200, open(p2, "rb").read(), mime)
                self.send(404, "not found", "text/plain")
            except Exception as e:  # keep the server alive for the screenshot run
                print("mock error:", p, repr(e)); self.send(500, repr(e), "text/plain")
    return H


if __name__ == "__main__":
    ap = argparse.ArgumentParser(); ap.add_argument("--port", type=int, default=8765); ap.add_argument("--fixture", default=os.path.join(HERE, "fixture.json"))
    a = ap.parse_args()
    model = Model(json.load(open(a.fixture)))
    ThreadingHTTPServer(("127.0.0.1", a.port), make_handler(model)).serve_forever()
