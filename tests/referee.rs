//! End-to-end tests with ffmpeg as an INDEPENDENT referee: acxorcist does the
//! conversion with zero ffmpeg involvement, then ffmpeg measures the output and
//! must confirm it is ACX-compliant. Skips gracefully when ffmpeg is absent.

use acxorcist::audio::convert_file;
use std::path::Path;
use std::process::Command;

fn have(bin: &str) -> bool {
    Command::new(bin).arg("-version").output().is_ok()
}

/// Synthesize a quiet 10 s tone MP3 with ffmpeg (test input only).
fn make_tone(path: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i", "sine=frequency=300:duration=10",
            "-filter:a", "volume=-30dB",
            "-c:a", "libmp3lame", "-b:a", "128k", "-ar", "44100", "-ac", "2",
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// ffmpeg volumedetect -> (mean_db, max_db).
fn ffmpeg_measure(path: &Path) -> (f32, f32) {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-i"])
        .arg(path)
        .args(["-af", "volumedetect", "-f", "null", "/dev/null"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    let grab = |key: &str| -> f32 {
        text.lines()
            .find(|l| l.contains(key))
            .and_then(|l| l.split(&format!("{key}:")).nth(1))
            .and_then(|s| s.trim().split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(f32::NAN)
    };
    (grab("mean_volume"), grab("max_volume"))
}

fn ffprobe_field(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", entries, "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn native_output_is_acx_compliant_per_ffmpeg() {
    if !have("ffmpeg") || !have("ffprobe") {
        eprintln!("SKIP: ffmpeg/ffprobe not installed");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("chapter.mp3");
    let dst = dir.path().join("out.mp3");
    make_tone(&src);

    // Convert with the native pipeline — no ffmpeg involved here.
    let m = convert_file(&src, &dst).unwrap();

    // 1. Independent referee: ffmpeg's own loudness measurement of our output.
    let (mean, max) = ffmpeg_measure(&dst);
    assert!(mean >= -23.0 && mean <= -18.0, "ffmpeg says mean {mean} dB, outside ACX window");
    assert!(max <= -3.0, "ffmpeg says peak {max} dB, above -3");

    // 2. Our own measurement must agree with the referee within ~1 dB.
    assert!((m.mean_db - mean).abs() < 1.0, "native {} vs ffmpeg {} mean", m.mean_db, mean);

    // 3. Format is exactly ACX: MP3, 44100 Hz, stereo, 192 kbps CBR.
    let codec = ffprobe_field(&dst, "stream=codec_name");
    let sr = ffprobe_field(&dst, "stream=sample_rate");
    let ch = ffprobe_field(&dst, "stream=channels");
    let br: i64 = ffprobe_field(&dst, "stream=bit_rate").parse().unwrap_or(0);
    assert_eq!(codec, "mp3");
    assert_eq!(sr, "44100");
    assert_eq!(ch, "2");
    assert!((br - 192_000).abs() <= 2_000, "bitrate {br} not ~192k CBR");
}
