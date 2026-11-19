# Factory presets of NoDAW Harmonic Space (scripts/write_presets.py writes presets/*.apfpreset).
# The 20 Space Recipes of the Spectral Space Designer prototype, converted from its knob
# positions (0..100) to plain units with the prototype's own mappings:
#   amp 0.1 + v/100*2.9 (linear)   hpf 20 + 4.8v Hz   lpf 2000 + 200v Hz (100 = open)
#   delay wet v %, time 0.1 + 0.9 v/100 s, feedback 30 %   reverb wet v/150   sub v/10 dB @ 80 Hz
#   flanger wet v %, rate 0.2 + 2 v/100 Hz   main (v/50)^2   EQ gain (v-50)*0.24 dB
#   EQ Q 0.3 + 9.7 v/100 (the prototype ignores Q on the 60 Hz / 16 kHz shelves)
import math

PLUGIN = "com.nodaw.harmonic-space"
FREQS = [60, 200, 800, 2000, 4000, 8000, 16000]


def recipe(name, family, desc, amp, hpf, lpf, delay, reverb, sub, flanger, main, eq_gain, eq_q, moods):
    p = {}
    a = 20 * math.log10(0.1 + amp / 100 * 2.9)
    if abs(a) > 0.05: p["amp"] = round(a, 2)
    if hpf > 0: p["hpf"] = round(20 + hpf * 4.8, 1)
    if lpf < 100: p["lpf"] = round(2000 + lpf * 200)
    if delay > 0:
        p["dly_mix"] = delay
        p["dly_time"] = round((0.1 + delay / 100 * 0.9) * 1000)
    if reverb > 0: p["rev_mix"] = round(reverb / 1.5, 1)
    if sub > 0: p["sub"] = round(sub / 10, 1)
    if flanger > 0:
        p["fl_mix"] = flanger
        p["fl_rate"] = round(0.2 + flanger / 100 * 2, 2)
    o = 40 * math.log10(main / 50)
    if abs(o) > 0.05: p["out_gain"] = round(o, 2)
    for i, (g, q) in enumerate(zip(eq_gain, eq_q)):
        n = i + 1
        db = (g - 50) * 0.24
        if abs(db) > 1e-6: p[f"b{n}_gain"] = round(db, 2)
        if n not in (1, 7) and q != 7: p[f"b{n}_q"] = round(0.3 + q / 100 * 9.7, 2)
    return (name, family, family, moods, desc, p)


PRESETS = [
    ("Init", "Utility", "Default", ["Clean"], "Everything flat and every effect out: a clean starting point", {}),
    recipe("Air Lift", "Vocals", "Gentle 8k/16k air with a mild 80 Hz rumble cut. Low end stays flat.", 31, 13, 100, 0, 6, 0, 0, 50, [50, 50, 50, 52, 55, 60, 64], [7] * 7, ["Bright", "Airy"]),
    recipe("De-Box", "Vocals", "Scoops boxy 200-800 Hz buildup and opens the top a touch.", 33, 15, 100, 0, 0, 0, 0, 50, [50, 42, 38, 50, 52, 54, 56], [7, 12, 9, 7, 7, 7, 7], ["Clear"]),
    recipe("Phone Hook", "Vocals", "Telephone band (~300 Hz-3.4 kHz) with a pushed 800 Hz-2 kHz honk.", 40, 58, 7, 0, 4, 0, 0, 54, [30, 40, 62, 66, 54, 40, 35], [7, 9, 12, 10, 9, 7, 7], ["Lo-Fi", "Narrow"]),
    recipe("Ad-lib Throw", "Vocals", "Brighter, thinner ad-lib thrown back with a long echo and hall wash.", 31, 25, 92, 38, 40, 0, 0, 46, [44, 44, 48, 56, 58, 60, 60], [7] * 7, ["Wide", "Wet"]),
    recipe("Whisper Double", "Vocals", "Soft, airy double: lower level, presence lift, light echo and room.", 24, 30, 85, 12, 18, 0, 6, 48, [44, 46, 48, 54, 60, 58, 56], [7, 7, 7, 7, 8, 7, 7], ["Soft", "Airy"]),
    recipe("808 Weight", "Low end", "+6 dB sub shelf, top rolled to ~8 kHz, small 800 Hz dip for clean weight.", 31, 2, 30, 0, 0, 60, 0, 44, [58, 50, 42, 48, 50, 50, 50], [7, 7, 10, 7, 7, 7, 7], ["Deep", "Heavy"]),
    recipe("Sub Glue", "Low end", "Moderate sub with a touch of 60/200 Hz and a tiny short echo to glue.", 31, 3, 70, 4, 0, 35, 0, 46, [55, 54, 48, 50, 50, 50, 50], [7, 6, 7, 7, 7, 7, 7], ["Warm"]),
    recipe("Kick Room", "Low end", "Short room and slight echo, 800 Hz box cut, 2-4 kHz beater punch.", 33, 6, 60, 6, 18, 15, 0, 48, [55, 46, 44, 56, 54, 50, 50], [7, 12, 9, 12, 10, 7, 7], ["Punchy"]),
    recipe("Tight Bass", "Low end", "HPF ~68 Hz, controlled sub, mud cut at 200/800 Hz. No effects.", 31, 10, 45, 0, 0, 20, 0, 50, [52, 40, 44, 52, 50, 50, 50], [7, 12, 10, 7, 7, 7, 7], ["Tight"]),
    recipe("Small Booth", "Space", "Low, short reverb and a hint of echo, like a tight vocal booth.", 31, 12, 85, 3, 12, 0, 0, 50, [50, 48, 50, 52, 52, 52, 52], [7] * 7, ["Intimate"]),
    recipe("Dark Hall", "Space", "Big reverb wash with the top cut at ~6 kHz and less air.", 31, 18, 20, 10, 85, 0, 0, 44, [48, 48, 50, 48, 44, 38, 34], [7] * 7, ["Dark", "Huge"]),
    recipe("Slapback Rap", "Space", "One quick ~0.2 s slap, almost no reverb, a bit of 2-4 kHz edge.", 31, 18, 90, 14, 2, 0, 0, 50, [48, 48, 48, 54, 54, 52, 52], [7] * 7, ["Tight", "Edgy"]),
    recipe("Ping-Pong Hook", "Space", "Long ~0.6 s echo bouncing left and right, and a big reverb for hooks.", 31, 22, 88, 55, 45, 0, 0, 42, [46, 46, 48, 54, 56, 56, 56], [7] * 7, ["Wide", "Wet"]),
    recipe("Lo-fi Room", "Character", "Muffled ~4.4 kHz top, thin lows, low-mid bump, light wobble and room.", 34, 30, 12, 6, 28, 0, 15, 50, [46, 54, 56, 50, 44, 40, 38], [7] * 7, ["Lo-Fi", "Dark"]),
    recipe("Tape Wobble", "Character", "Flanger warble (~1 Hz) with a soft, rolled-off top and a little warmth.", 31, 10, 32, 0, 8, 10, 45, 44, [52, 52, 50, 48, 46, 44, 42], [7] * 7, ["Vintage", "Wobbly"]),
    recipe("Syrup Haze", "Character", "Slow flanger, deep reverb, dark ~5.6 kHz LPF. Pairs with NoDAW Screw.", 31, 8, 18, 16, 55, 25, 20, 42, [54, 54, 50, 46, 44, 40, 38], [7] * 7, ["Hazy", "Dark"]),
    recipe("Radio Break", "Character", "Band-limited radio (~260 Hz-4.4 kHz), hot mids, amp pushed with output trimmed.", 70, 50, 12, 0, 6, 0, 0, 36, [35, 42, 64, 62, 52, 40, 38], [7, 9, 10, 9, 7, 7, 7], ["Lo-Fi", "Narrow"]),
    recipe("Harsh Sample Tamer", "Fix-it", "Broad cuts across 2-8 kHz plus a mild 13 kHz LPF to calm brittle samples.", 31, 6, 55, 0, 0, 0, 0, 52, [50, 50, 50, 44, 40, 42, 46], [7, 7, 7, 9, 10, 9, 7], ["Smooth"]),
    recipe("Mud Cut", "Fix-it", "Clears 200-800 Hz mud with a slight ~87 Hz HPF. Top stays open.", 33, 14, 100, 0, 0, 0, 0, 50, [50, 40, 42, 51, 50, 51, 51], [7, 10, 9, 7, 7, 7, 7], ["Clear"]),
    recipe("Wide-but-Mono-Safe", "Fix-it", "Width from a moderate stereo reverb and a light echo only. No flanger, so it holds up in mono.", 31, 12, 92, 8, 20, 0, 0, 48, [50, 48, 50, 52, 52, 54, 54], [7] * 7, ["Wide", "Safe"]),
]
# true ping-pong for the hook (the prototype's delay was mono)
PRESETS[13][5]["dly_pp"] = True
