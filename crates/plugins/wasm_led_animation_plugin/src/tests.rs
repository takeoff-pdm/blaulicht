use super::*;
fn frame(engine: &mut Engine, dt: f64, bpm: f64) -> Vec<Rgb> {
    engine.frame(dt, bpm, 1.0, false, COUNT).unwrap()
}
#[test]
fn all_effects_are_bounded_deterministic_distinct_and_animated() {
    let mut fingerprints = Vec::new();
    for effect in 0..10 {
        let mut a = Engine::default();
        a.settings.effect = effect;
        let mut b = Engine::default();
        b.settings.effect = effect;
        let mut samples = Vec::new();
        for _ in 0..100 {
            let actual = frame(&mut a, 0.137, 120.0);
            assert_eq!(actual, frame(&mut b, 0.137, 120.0));
            assert!(actual
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
            samples.push(actual);
        }
        assert!(
            samples.windows(2).any(|w| w[0] != w[1]),
            "effect {effect} frozen"
        );
        assert!(
            samples.iter().flatten().flatten().any(|v| *v > 0.2),
            "effect {effect} dark"
        );
        assert!(!fingerprints.contains(&samples));
        fingerprints.push(samples);
    }
}
#[test]
fn tube_boundaries_and_reversal() {
    let mut normal = Engine::default();
    let mut reversed = Engine::default();
    reversed.settings.reversed[2] = true;
    let a = frame(&mut normal, 1.0, 120.0);
    let b = frame(&mut reversed, 1.0, 120.0);
    for i in 0..COUNT {
        assert_eq!(
            b[i],
            a[if i / PIXELS == 2 {
                2 * PIXELS + PIXELS - 1 - i % PIXELS
            } else {
                i
            }]
        );
    }
    assert_ne!(&a[..PIXELS], &a[PIXELS..2 * PIXELS]);
}
#[test]
fn tempo_speed_pause_and_invalid_selection() {
    for bpm in [60.0, 120.0, 180.0] {
        let mut e = Engine::default();
        frame(&mut e, 1.0, bpm);
        assert_eq!(e.beats, bpm / 60.0);
    }
    for bpm in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(tempo(bpm), (120.0, true));
    }
    let mut e = Engine::default();
    frame(&mut e, 1.0, 60.0);
    frame(&mut e, 1.0, 180.0);
    assert_eq!(e.beats, 4.0);
    assert!(e.frame(5.0, 120.0, 1.0, true, COUNT).is_none());
    for count in [0, 1, 99, 494, 496] {
        assert!(e.frame(5.0, 120.0, 1.0, false, count).is_none());
    }
    assert_eq!(e.beats, 4.0);
    e.settings.speed = 200;
    e.frame(1.0, 120.0, 0.5 * 0.5, false, COUNT);
    assert_eq!(e.beats, 6.0);
}
#[test]
fn switching_blends_current_frame_and_pause_freezes_fade() {
    let mut e = Engine::default();
    let old = frame(&mut e, 2.0, 120.0);
    e.settings.effect = 7;
    assert_eq!(old, frame(&mut e, 0.0, 120.0));
    e.frame(10.0, 120.0, 1.0, true, COUNT);
    assert_eq!(e.fade_seconds, 0.0);
    frame(&mut e, 0.2, 120.0);
    let middle = e.last.clone();
    e.settings.effect = 1;
    assert_eq!(middle, frame(&mut e, 0.0, 120.0));
    frame(&mut e, 0.4, 120.0);
    assert!(e.fade_from.is_none());
}
#[test]
fn persistence_defaults_and_isolation() {
    let mut a = Engine::default();
    a.settings.effect = 8;
    a.settings.reversed[4] = true;
    a.settings.hue = 91;
    let raw = serde_json::to_string(&a.settings).unwrap();
    assert_eq!(Settings::load(&raw), a.settings);
    assert_eq!(Settings::load("broken"), Settings::default());
    let s = Settings::load(r#"{"effect":255,"speed":255}"#);
    assert_eq!(s.effect, 9);
    assert_eq!(s.speed, 200);
    assert_eq!(s.brightness, 255);
    let b = Engine::default();
    frame(&mut a, 1.0, 150.0);
    assert_eq!(b.beats, 0.0);
    assert_eq!(b.settings, Settings::default());
}
#[test]
fn elapsed_time_partition_and_color_roundtrip() {
    let mut a = Engine::default();
    let mut b = Engine::default();
    frame(&mut a, 1.0, 120.0);
    for _ in 0..100 {
        frame(&mut b, 0.01, 120.0);
    }
    for (x, y) in a.last.iter().flatten().zip(b.last.iter().flatten()) {
        assert!((x - y).abs() < 1e-10);
    }
    for h in 0..360 {
        let rgb = hsv(h as f64, 0.7, 0.8);
        let (hh, s, v) = rgb_hsv(rgb);
        assert!((h as f64 - hh).abs() < 1e-9);
        assert!((s - 0.7).abs() < 1e-9);
        assert!((v - 0.8).abs() < 1e-9);
    }
    a.settings.brightness = 0;
    assert!(frame(&mut a, 0.1, 120.0)
        .iter()
        .flatten()
        .all(|v| *v == 0.0));
}

#[test]
fn real_clock_handles_slow_ticks_pause_and_wrap() {
    let mut clock = MotionClock::default();
    assert_eq!(clock.seconds(100, 12), 0.012);
    assert_eq!(clock.seconds(150, 12), 0.05);
    assert_eq!(clock.seconds(150, 12), 0.0);
    assert_eq!(clock.seconds(10150, 12), 0.25);
    assert_eq!(clock.seconds(10170, 12), 0.02);
    let mut clock = MotionClock::default();
    clock.seconds(u32::MAX - 9, 12);
    assert_eq!(clock.seconds(10, 12), 0.02);
}
