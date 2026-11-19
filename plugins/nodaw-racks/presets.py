# Factory presets of NoDAW Racks (scripts/write_presets.py writes presets/*.apfpreset).
PLUGIN = "com.nodaw.racks"
PRESETS = [
    ("Init", "Utility", "Default", ["Clean"], "Every module in, nothing pushed: a gentle tape touch and a safety limiter", {}),
    ("Analog Glue", "Mix Bus", "Glue", ["Warm", "Cohesive"], "Tape drive, a little low-end weight and air, limiter just kissing", {
        "sat_curve": "Tape", "sat_drive": 9, "sat_tone": 0.5, "eq1_gain": 1.5, "eq1_freq": 70, "eq7_gain": 1.5, "eq7_freq": 11000,
        "lim_gain": 2, "lim_ceiling": -0.8, "lim_release": 180}),
    ("Tape Warmth", "Character", "Tape", ["Warm", "Vintage"], "Pushed tape with rounded highs", {
        "sat_curve": "Tape", "sat_drive": 15, "sat_tone": -2.5, "sat_mix": 85, "eq6_gain": -1.5, "eq6_freq": 6500, "eq6_q": 0.7, "lim_on": False}),
    ("Tube Vocal", "Vocal", "Lead", ["Present", "Warm"], "Tube colour, mud cut, presence and air for a lead vocal", {
        "sat_curve": "Tube", "sat_drive": 10, "eq1_type": "Low Cut", "eq1_freq": 90, "eq1_q": 0.7, "eq3_gain": -3, "eq3_freq": 320, "eq3_q": 1.4,
        "eq5_gain": 2.5, "eq5_freq": 3200, "eq5_q": 1.1, "eq7_gain": 3, "eq7_freq": 12000, "sp_width": 90, "lim_gain": 3, "lim_ceiling": -1}),
    ("Diode Crunch Drums", "Drums", "Bus", ["Aggressive", "Punchy"], "Diode grit blended in parallel, scooped mids, hard limit", {
        "sat_curve": "Diode", "sat_drive": 20, "sat_mix": 55, "eq2_gain": 2.5, "eq2_freq": 90, "eq2_q": 1.2, "eq4_gain": -3, "eq4_freq": 800,
        "eq4_q": 0.9, "eq6_gain": 2, "eq6_freq": 5500, "lim_gain": 6, "lim_release": 60, "order": "EQ > SAT > SPACE > LIM"}),
    ("Wide Master", "Master", "Stereo", ["Wide", "Open"], "Gentle width with mono-safe lows and a transparent ceiling", {
        "sat_drive": 4, "sp_width": 135, "sp_mono": 140, "eq7_gain": 1.2, "lim_gain": 3, "lim_ceiling": -1, "lim_release": 220}),
    ("Mono Bass Club", "Master", "Club", ["Solid", "Tight"], "Everything under 180 Hz in mono, tight low end", {
        "sp_mono": 180, "sp_width": 110, "eq1_gain": 2, "eq1_freq": 55, "eq2_gain": -2, "eq2_freq": 250, "lim_gain": 4}),
    ("Air And Weight", "Mix Bus", "Tone", ["Bright", "Deep"], "Low shelf weight and an airy top", {
        "eq1_gain": 3, "eq1_freq": 75, "eq7_gain": 4, "eq7_freq": 14000, "eq3_gain": -1.5, "eq3_freq": 450, "sat_drive": 5}),
    ("Loud Reactor", "Master", "Loud", ["Loud", "Dense"], "Hard-pushed limiter with adaptive release for dense masters", {
        "sat_curve": "Tape", "sat_drive": 8, "lim_gain": 10, "lim_ceiling": -0.5, "lim_release": 90, "lim_adaptive": True}),
    ("Haas Guitar", "Guitar", "Stereo", ["Wide"], "Haas widening for a mono guitar, low cut, presence", {
        "sp_haas": 14, "sp_width": 120, "eq1_type": "Low Cut", "eq1_freq": 110, "eq5_gain": 2, "eq5_freq": 2500, "sat_curve": "Tube", "sat_drive": 8, "lim_on": False}),
    ("Lo-Fi Radio", "Character", "Lo-Fi", ["Dark", "Gritty"], "Band-limited and gritty, like a small speaker", {
        "sat_curve": "Diode", "sat_drive": 16, "eq1_type": "Low Cut", "eq1_freq": 400, "eq1_q": 0.9, "eq7_type": "High Cut", "eq7_freq": 3500,
        "eq7_q": 0.9, "eq4_gain": 4, "eq4_freq": 1500, "sp_width": 20, "lim_gain": 6}),
    ("Gentle Polish", "Master", "Polish", ["Clean", "Smooth"], "Barely-there saturation, a smile curve and a light ceiling", {
        "sat_drive": 3, "sat_mix": 60, "eq1_gain": 1, "eq7_gain": 1.5, "eq4_gain": -0.8, "lim_gain": 1.5, "lim_release": 250}),
]
