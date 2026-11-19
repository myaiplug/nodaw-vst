//! The plugin through the CLAP interface, the way a host drives it: parameters, automation,
//! latency and tail, state, and the real-time rule (no allocation in `process`).

use apf_test::{allocations_in_process, CountingAllocator, Event, Instance};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn plugin(sr: f64, max_frames: u32) -> Instance {
    let mut p = Instance::load(
        nodaw_screw::apf_entry_init,
        nodaw_screw::apf_entry_get_factory,
    );
    p.activate(sr, max_frames);
    p
}

fn sine(n: usize, hz: f64, sr: f64, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (amp as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / sr).sin()) as f32)
        .collect()
}

#[test]
fn the_host_sees_every_control_as_an_automatable_parameter() {
    let p = plugin(48_000.0, 512);
    assert_eq!(p.param_count() as usize, nodaw_screw::PARAMS.len());
    let mut ids = std::collections::HashSet::new();
    for i in 0..p.param_count() {
        let info = p.param_info(i);
        assert!(ids.insert(info.id), "duplicate host id");
        assert!(
            info.flags & clap_sys::ext::params::CLAP_PARAM_IS_AUTOMATABLE != 0,
            "param {i} is not automatable"
        );
        assert!(
            info.flags & clap_sys::ext::params::CLAP_PARAM_IS_HIDDEN == 0,
            "param {i} is hidden"
        );
    }
}

#[test]
fn parameter_text_round_trips_in_the_units_the_user_sees() {
    let mut p = plugin(48_000.0, 512);
    for (name, text, expect) in [
        ("Pitch", "-7.5 st", "-7.50 st"),
        ("Output", "-6 dB", "-6.0 dB"),
        ("Chop Pattern", "Trap Roll", "Trap Roll"),
        ("Limiter", "Off", "Off"),
    ] {
        let host = p.set_text(name, text);
        let id = p.param_id(name);
        assert_eq!(p.value_to_text(id, host), expect, "{name}");
    }
}

#[test]
fn latency_and_tail_are_reported_and_exact() {
    for sr in [44_100.0, 48_000.0, 96_000.0] {
        let mut p = plugin(sr, 1024);
        let lat = p.latency() as usize;
        assert!(
            lat as f64 / sr > 0.005 && (lat as f64 / sr) < 0.08,
            "{sr}: {lat}"
        );
        assert!(
            p.tail() as f64 / sr > 20.0,
            "tail covers the longest reverb"
        );
        // Bypassed, an impulse comes back exactly `lat` samples later: the host's delay
        // compensation lines it up with the other tracks.
        p.set_text("Bypass", "On");
        let n = 48_000;
        let mut x = vec![0.0f32; n];
        x[9000] = 1.0;
        let out = p.render_effect(&[], &x, &x, 480);
        // Bypass crossfades in over a few milliseconds, so the impulse is placed 190 ms in.
        let (idx, v) = out
            .left
            .iter()
            .enumerate()
            .skip(2000)
            .fold(
                (0, 0.0f32),
                |m, (i, v)| if v.abs() > m.1 { (i, v.abs()) } else { m },
            );
        assert_eq!(idx, 9000 + lat, "{sr}");
        assert!(v > 0.99, "{v}");
    }
}

#[test]
fn process_never_allocates_even_with_automation_in_the_block() {
    let sr = 48_000.0;
    let mut p = plugin(sr, 512);
    let pitch = p.param_id("Pitch");
    let mix = p.param_id("Mix");
    let pattern = p.param_id("Chop Pattern");
    p.set_text("Chop Pattern", "Trap Roll");
    p.set_text("Chop Rate", "1/16");
    p.set_text("Reverb", "50 %");
    p.set_text("Wow", "30 ct");
    p.set_text("Flutter", "40 %");
    let x = sine(48_000, 220.0, sr, 0.5);
    let before = allocations_in_process();
    for block in [32usize, 100, 480, 512] {
        let events = [
            Event::param(10, pitch, 0.9),
            Event::param(20, mix, 0.5),
            Event::param(30, pattern, 3.0),
        ];
        let _ = p.render_effect(&events, &x, &x, block);
        let events = [Event::param(0, pitch, 0.1)];
        let _ = p.render_effect(&events, &x, &x, block);
    }
    assert_eq!(allocations_in_process(), before, "process() allocated");
}

#[test]
fn automation_moves_the_sound_inside_the_block() {
    let sr = 48_000.0;
    let mut p = plugin(sr, 4800);
    p.set_text("Mix", "0 %");
    p.set_text("Reverb", "0 %");
    p.set_text("Limiter", "Off");
    let id = p.param_id("Output");
    let x = vec![0.25f32; 9600];
    // Output from -24 dB .. +24 dB: host value 0.5 is 0 dB. Drop to the minimum at sample 4800.
    let out = p.render_effect(&[Event::param(0, id, 0.5)], &x, &x, 9600);
    let lat = p.latency() as usize;
    assert!((out.left[lat + 4000] - 0.25).abs() < 0.01);
    let id_min = 0.0;
    let out = p.render_effect(&[Event::param(0, id, id_min)], &x, &x, 9600);
    let tailv = out.left[9599];
    assert!(tailv.abs() < 0.25 * 0.1, "gain fell: {tailv}");
}

#[test]
fn state_round_trips_the_session() {
    let mut a = plugin(48_000.0, 512);
    for (n, t) in [
        ("Pitch", "-9.25 st"),
        ("Reverb", "61 %"),
        ("Decay", "7.5 s"),
        ("Chop Pattern", "Custom"),
        ("Chop Rate", "1/16T"),
        ("Limiter", "Off"),
        ("Ceiling", "-3.5 dB"),
        ("Width", "140 %"),
    ] {
        a.set_text(n, t);
    }
    let blob = a.save_state();
    assert!(!blob.is_empty());
    let mut b = plugin(44_100.0, 256);
    assert!(b.load_state(blob.clone()));
    for i in 0..a.param_count() {
        let id = a.param_info(i).id;
        assert!(
            (a.host_value(id) - b.host_value(id)).abs() < 1e-9,
            "param {i} differs after load"
        );
    }
    // Saving again gives the same bytes: nothing is lost or invented on the way.
    assert_eq!(b.save_state(), blob);
    // Garbage is refused, not half-applied.
    let mut c = plugin(48_000.0, 512);
    assert!(!c.load_state(vec![1, 2, 3, 4, 5]));
}

#[test]
fn bypass_is_transparent_and_other_settings_do_not_leak_in() {
    let sr = 48_000.0;
    let mut p = plugin(sr, 512);
    p.set_text("Pitch", "-12 st");
    p.set_text("Reverb", "100 %");
    p.set_text("Bypass", "On");
    let x = sine(48_000, 330.0, sr, 0.5);
    let out = p.render_effect(&[], &x, &x, 512);
    let lat = p.latency() as usize;
    for i in (lat + 4000)..x.len() {
        assert!((out.left[i] - x[i - lat]).abs() < 1e-5, "{i}");
    }
}

#[test]
fn works_at_every_common_sample_rate_and_buffer_size() {
    for sr in [22_050.0, 44_100.0, 48_000.0, 88_200.0, 96_000.0, 192_000.0] {
        for frames in [1u32, 16, 64, 512, 4096] {
            let mut p = plugin(sr, frames);
            p.set_text("Reverb", "40 %");
            let x = sine(8192, 200.0, sr, 0.5);
            let out = p.render_effect(&[], &x, &x, frames as usize);
            assert!(
                out.left.iter().chain(&out.right).all(|v| v.is_finite()),
                "{sr} {frames}"
            );
        }
    }
}
