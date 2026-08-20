//! Unit tests for the DSP building blocks (no external tools needed).

use acxorcist::audio::{apply_gain, measure, Audio};

fn tone(frames: usize, amp: f32) -> Audio {
    Audio {
        sample_rate: 44_100,
        frames: (0..frames).map(|_| [amp, amp]).collect(),
    }
}

#[test]
fn measure_matches_rms_and_peak_formulas() {
    let a = tone(1000, 0.5);
    let m = measure(&a);
    // RMS of a constant 0.5 = 0.5 -> 20*log10(0.5) = -6.02 dB (mean == peak here).
    assert!((m.mean_db - (-6.02)).abs() < 0.05, "mean {}", m.mean_db);
    assert!((m.peak_db - (-6.02)).abs() < 0.05, "peak {}", m.peak_db);
}

#[test]
fn gain_scales_linearly() {
    let mut a = tone(10, 0.25);
    apply_gain(&mut a, 2.0);
    assert!(a.frames.iter().all(|f| (f[0] - 0.5).abs() < 1e-6));
}

#[test]
fn limiter_clamps_peaks_above_ceiling() {
    use acxorcist::audio::limit;
    // Mostly quiet with one loud spike well above the ceiling.
    let mut a = tone(2000, 0.05);
    a.frames[1000] = [0.95, 0.95];
    limit(&mut a, 0.668, 44_100);
    let peak = a
        .frames
        .iter()
        .map(|f| f[0].abs().max(f[1].abs()))
        .fold(0.0f32, f32::max);
    assert!(peak <= 0.668 + 1e-4, "peak after limit = {peak}");
}

#[test]
fn limiter_leaves_quiet_signal_unchanged() {
    use acxorcist::audio::limit;
    let mut a = tone(1000, 0.2); // all below the 0.668 ceiling
    let before = a.frames.clone();
    limit(&mut a, 0.668, 44_100);
    assert!(a
        .frames
        .iter()
        .zip(&before)
        .all(|(x, y)| (x[0] - y[0]).abs() < 1e-6));
}

#[test]
fn pad_adds_exact_room_tone() {
    use acxorcist::audio::pad_silence;
    let mut a = tone(44_100, 0.3); // 1 s of tone
    pad_silence(&mut a, 0.5, 2.0);
    // 0.5 s + 1 s + 2 s = 3.5 s at 44.1k.
    assert_eq!(a.frames.len(), (3.5 * 44_100.0) as usize);
    assert_eq!(a.frames[0], [0.0, 0.0]); // head silence
    assert_eq!(*a.frames.last().unwrap(), [0.0, 0.0]); // tail silence
}
