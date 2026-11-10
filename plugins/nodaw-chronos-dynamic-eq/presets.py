# Factory presets of NoDAW Chronos Dynamic EQ (scripts/write_presets.py writes presets/*.apfpreset).
PLUGIN = "com.nodaw.chronos-dynamic-eq"


def band(n, **kw):
    return {f"b{n}_{k}": v for k, v in kw.items()}


def merge(*ds):
    out = {}
    for d in ds:
        out.update(d)
    return out


PRESETS = [
    ("Init", "Utility", "Default", ["Clean"], "Seven flat static bands: a clean start", {}),
    ("Vocal De-Esser", "Vocal", "De-Ess", ["Smooth"], "A narrow 6.5 kHz band that only cuts on the S sounds", merge(
        band(6, freq=6500, q=2.5, mode="Compress", thresh=-30, ratio=6, range=9, attack=0.5, release=60))),
    ("Tame The Boom", "Bass", "Control", ["Tight"], "Holds back a boomy 150 Hz only when it builds up", merge(
        band(2, freq=150, q=1.4, mode="Compress", thresh=-22, ratio=4, range=8, attack=8, release=180))),
    ("Kick Punch", "Drums", "Kick", ["Punchy"], "Upward expansion at 60 Hz on each hit, a static box cut at 400 Hz", merge(
        band(1, type="Bell", freq=60, q=1.2, mode="Expand", thresh=-26, ratio=3, range=5, attack=2, release=90),
        band(3, gain=-3, freq=400, q=1.6))),
    ("De-Harsh Guitars", "Guitar", "Tone", ["Smooth"], "Dynamic cuts at 2.5 and 4 kHz that follow the pick", merge(
        band(5, freq=2600, q=1.8, mode="Compress", thresh=-28, ratio=4, range=6, attack=3, release=120),
        band(6, freq=4200, q=2, mode="Compress", thresh=-30, ratio=4, range=5, attack=3, release=120))),
    ("Mix Bus Smooth", "Mix Bus", "Glue", ["Smooth", "Gentle"], "Gentle low-mid and upper-mid control, a little static air", merge(
        band(3, freq=300, q=0.9, mode="Compress", thresh=-20, ratio=2, range=3, attack=15, release=250),
        band(5, freq=3000, q=0.9, mode="Compress", thresh=-24, ratio=2, range=3, attack=10, release=200),
        band(7, gain=1.5, freq=12000))),
    ("Mastering Air", "Master", "Air", ["Bright", "Open"], "A high shelf that opens up when the top end is there", merge(
        band(7, freq=10000, mode="Expand", thresh=-38, ratio=2, range=3, attack=20, release=300),
        band(1, freq=40, gain=0.8))),
    ("Bass Control", "Bass", "Shelf", ["Solid"], "A low shelf that compresses the lows so the bass sits steady", merge(
        band(1, freq=120, mode="Compress", thresh=-20, ratio=3, range=6, attack=20, release=200))),
    ("Podcast Voice", "Voice", "Spoken", ["Clear", "Warm"], "Proximity control, a presence lift that rides with the voice and a de-esser", merge(
        band(1, freq=110, mode="Compress", thresh=-24, ratio=3, range=6, attack=15, release=200),
        band(3, gain=-2, freq=350, q=1.2),
        band(5, freq=3000, q=1, mode="Expand", thresh=-34, ratio=2, range=3, attack=10, release=150),
        band(6, freq=7000, q=2.2, mode="Compress", thresh=-30, ratio=5, range=8, attack=0.5, release=60))),
    ("Snare Snap", "Drums", "Snare", ["Crisp"], "Upward expansion of the 5 kHz crack, control of the 900 Hz ring", merge(
        band(4, freq=900, q=3, mode="Compress", thresh=-24, ratio=4, range=6, attack=1, release=80),
        band(6, freq=5000, q=1.2, mode="Expand", thresh=-30, ratio=3, range=6, attack=0.5, release=70))),
    ("Vocal Presence Pop", "Vocal", "Lead", ["Present"], "The 3 kHz presence comes forward on loud phrases, mud stays tucked", merge(
        band(3, freq=250, q=1.2, mode="Compress", thresh=-24, ratio=3, range=4, attack=10, release=150),
        band(5, freq=3200, q=1.1, mode="Expand", thresh=-30, ratio=3, range=4, attack=5, release=120))),
    ("Resonance Hunter", "Fix-it", "Surgical", ["Surgical"], "Three narrow dynamic notches on common room resonances", merge(
        band(2, freq=180, q=8, mode="Compress", thresh=-34, ratio=10, range=12, attack=5, release=150),
        band(3, freq=420, q=8, mode="Compress", thresh=-34, ratio=10, range=12, attack=5, release=150),
        band(4, freq=1100, q=8, mode="Compress", thresh=-36, ratio=10, range=12, attack=5, release=150))),
]
