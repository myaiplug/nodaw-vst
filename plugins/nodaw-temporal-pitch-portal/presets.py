# Factory presets of NoDAW Temporal Pitch Portal (scripts/write_presets.py writes presets/*.apfpreset).
# The prototype's 5 Warp Zones and 5 ScrewAI Flavors (time %, pitch st as in its docs). The
# prototype coupled time and pitch through playbackRate; here they are independent, so the
# Warp Zones use the pitch-keeping Stretch warp and the stated pitch, and the Flavors add the
# drift, tone and echo that make them sound "screwed".
PLUGIN = "com.nodaw.temporal-pitch-portal"


def p(**kw):
    return {k: v for k, v in kw.items()}


PRESETS = [
    ("Init", "Utility", "Default", ["Clean"], "Straight through: 100 % time, 0 st", {}),
    ("Classic Portal", "Warp Zones", "Neutral", ["Clean"], "Neutral portal: 100 % time, 0 st, a whisper of drift", p(drift=3.0, drift_rate=0.4)),
    ("Deep Rift", "Warp Zones", "Down", ["Dark", "Slow"], "Slowed and pitched down: 65 % time, -5 st", p(time=65, pitch=-5.0, tone=-20, glide=120)),
    ("Zero Gravity", "Warp Zones", "Up", ["Bright", "Fast"], "Sped up and pitched up: 150 % time, +7 st", p(time=150, pitch=7.0, tone=15, warp_sync="1/2 Bar")),
    ("Wormhole Clamp", "Warp Zones", "Down", ["Dark", "Heavy"], "Moderately slowed and deep: 80 % time, -8 st", p(time=80, pitch=-8.0, tone=-30, echo_mix=12, echo_time=500, echo_fb=40)),
    ("Glass Phase Portal", "Warp Zones", "Up", ["Bright", "Airy"], "Slightly sped up with a high, glassy pitch: 120 % time, +3 st", p(time=120, pitch=3.0, tone=35, drift=8.0, drift_rate=3.5, echo_mix=18, echo_time=250, echo_fb=30)),
    ("Slowed Portal", "ScrewAI Flavors", "Slowed", ["Laid back"], "Laid-back vibe: 70 % time, -3 st", p(time=70, pitch=-3.0, tone=-25, drift=6.0, drift_rate=0.3)),
    ("Double Cup Dimension", "ScrewAI Flavors", "Chopped", ["Syrupy", "Dark"], "Deep chopped sound: 60 % time, -6 st on tape", p(time=60, pitch=-6.0, warp_mode="Tape", tone=-40, drift=10.0, drift_rate=0.25, echo_mix=15, echo_time=750, echo_fb=45)),
    ("Houston Rift", "ScrewAI Flavors", "Screwed", ["Classic"], "The classic screw: 75 % time, -4 st", p(time=75, pitch=-4.0, tone=-30, drift=4.0, drift_rate=0.2, glide=200)),
    ("Mane Hold Time", "ScrewAI Flavors", "Slowed", ["Subtle"], "Subtle slow: 85 % time, -2 st", p(time=85, pitch=-2.0, tone=-10)),
    ("Astro Portals", "ScrewAI Flavors", "Sped up", ["Bright", "Fast"], "Sped up and bright: 130 % time, +5 st", p(time=130, pitch=5.0, tone=30, echo_mix=20, echo_time=187, echo_fb=35, warp_sync="1/2 Bar")),
    ("Tape Half-Time", "Warp Zones", "Tape", ["Vintage"], "Half-time on a varispeed tape: an octave down, every bar", p(time=50, warp_mode="Tape", tone=-15)),
    ("Seasick Drift", "ScrewAI Flavors", "Wobble", ["Wobbly", "Lo-Fi"], "Pitch stays put but wanders: 35 cents of drift at 1.3 Hz", p(drift=35.0, drift_rate=1.3, tone=-20, echo_mix=10, echo_time=320, echo_fb=25)),
    ("Octave Echo Down", "Warp Zones", "Down", ["Huge", "Wet"], "An octave down with a long dark echo", p(pitch=-12.0, tone=-35, echo_mix=35, echo_time=600, echo_fb=55, mix=70)),
]
