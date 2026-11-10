# Factory presets of NoDAW Chronos Weaver (scripts/write_presets.py writes presets/*.apfpreset).
# The Snapshot presets need a capture (the Capture B button) before they morph; until then they
# act on the live signal only (the cloud and freeze still work).
PLUGIN = "com.nodaw.chronos-weaver"


def p(**kw):
    return dict(kw)


PRESETS = [
    ("Init", "Utility", "Default", ["Clean"], "Straight through: no morph, no freeze, no cloud", {}),
    ("Time Smear", "Morph", "Past", ["Hazy"], "Half-way to what played 250 ms ago: notes bleed into each other", p(morph=50, weave=250, cloud=25, grain=120)),
    ("Echoes Of Itself", "Morph", "Past", ["Wet", "Moving"], "The spectrum of one bar ago (at 120 BPM) breathes in and out with the LFO", p(morph=35, weave=2000, lfo_depth=70, lfo_sync="2 Bars")),
    ("Infinite Drone", "Freeze", "Hold", ["Huge", "Static"], "Freeze held forever with a slow cloud: press Freeze on a chord", p(freeze=True, sustain=100, cloud=30, grain=600, mix=85)),
    ("Frozen Choir", "Freeze", "Hold", ["Airy"], "A freeze that fades over ~25 s with scattered phases: choir-like", p(freeze=True, sustain=65, cloud=45, grain=300, scatter=40)),
    ("Granular Haze", "Cloud", "Grains", ["Hazy", "Soft"], "Medium grains at 60 % density", p(cloud=80, grain=180, density=60, scatter=30)),
    ("Glass Cloud", "Cloud", "Grains", ["Bright", "Sparkly"], "Short, sparse grains with full phase scatter", p(cloud=90, grain=25, density=25, scatter=100, mix=70)),
    ("Formant Swap", "Morph", "Snapshot", ["Hybrid"], "Cross-synthesis: the captured sound's envelope on the live one (capture B first)", p(morph=100, morph_mode="Cross", source="Snapshot")),
    ("Breathing Morph", "Morph", "Snapshot", ["Moving"], "Blend toward the capture and back every bar", p(morph=50, source="Snapshot", lfo_depth=100, lfo_sync="1 Bar", lfo_shape="Triangle")),
    ("Shattered Bar", "Cloud", "Rhythmic", ["Glitchy"], "Ramp LFO on a morph to the past with sparse grains", p(morph=40, weave=500, cloud=60, grain=60, density=35, scatter=60, lfo_depth=80, lfo_sync="1/2", lfo_shape="Ramp")),
    ("Ghost Pad", "Freeze", "Hold", ["Dark", "Wide"], "A long freeze under the dry signal", p(freeze=True, sustain=90, cloud=55, grain=900, scatter=20, mix=45)),
    ("Spectral Tremor", "Morph", "Past", ["Rhythmic"], "Fast synced morph against 20 ms ago: a spectral tremolo", p(morph=50, weave=20, lfo_depth=100, lfo_sync="1/4")),
]
