#!/usr/bin/env python3
"""Writes a plugin's factory presets (.apfpreset) from a compact table.

    python3 scripts/write_presets.py plugins/nodaw-racks/presets.py

The table file defines PLUGIN (the CLAP id) and PRESETS: a list of
(name, type, subtype, moods, description, {param: value}). Parameters a preset does not name
get their default when it loads (APF's preset rule), so each preset lists what it changes.
"""
import json, os, runpy, sys

table = runpy.run_path(sys.argv[1])
out_dir = os.path.join(os.path.dirname(sys.argv[1]), "presets")
os.makedirs(out_dir, exist_ok=True)
for name, typ, sub, mood, desc, params in table["PRESETS"]:
    doc = {"apf-preset": 1, "plugin": table["PLUGIN"], "name": name, "type": typ, "subtype": sub,
           "mood": mood, "description": desc, "author": "NoDAW", "params": params}
    with open(os.path.join(out_dir, f"{name}.apfpreset"), "w") as f:
        json.dump(doc, f, indent=2)
        f.write("\n")
print(f"{len(table['PRESETS'])} presets -> {out_dir}")
