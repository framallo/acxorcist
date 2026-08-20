//! acxorcist CLI — report / convert / verify, acting on the current directory.
//! One self-contained binary; no ffmpeg or other external tool at runtime.

use acxorcist::audio::{convert_file, decode_mp3, measure, Measure};
use anyhow::Result;
use std::path::{Path, PathBuf};

const USAGE: &str = "\
acxorcist — Convert MP3 files in this folder to ACX-compliant audio.

Usage (acts on the current working directory):
  acxorcist            Report current ACX compliance of the MP3s (no changes)
  acxorcist convert    Convert every *.mp3 here -> \"ACX Compliant/\"
  acxorcist verify     Report compliance of files already in \"ACX Compliant/\"

Self-contained: no ffmpeg or other external tool required.";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "report".into());
    let code = match mode.as_str() {
        "report" | "check" => run_report(&cwd()),
        "verify" => run_report(&cwd().join("ACX Compliant")),
        "convert" => run_convert(),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("Unknown command: {other}\nUsage: acxorcist [report|convert|verify]");
            std::process::exit(2);
        }
    };
    if let Err(e) = code {
        eprintln!("ERROR: {e:#}");
        std::process::exit(1);
    }
}

fn cwd() -> PathBuf {
    std::env::current_dir().expect("cwd")
}

fn mp3s_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("mp3")).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Print the same compliance line the reference tool prints.
fn report_line(m: &Measure) -> String {
    let rms_ok = if m.mean_db >= -23.0 && m.mean_db <= -18.0 { "OK" } else { "FAIL" };
    let peak_ok = if m.peak_db <= -3.0 { "OK" } else { "FAIL" };
    let len_ok = if m.duration_s <= 7200.0 { "OK" } else { "FAIL" };
    format!(
        "  RMS {:>6.1} dB [{:<4} | -23..-18]   Peak {:>6.1} dB [{:<4} | <=-3]   Len {:>6.0}s [{} | <=7200]",
        m.mean_db, rms_ok, m.peak_db, peak_ok, m.duration_s, len_ok
    )
}

fn run_report(dir: &Path) -> Result<()> {
    println!("ACX compliance report for MP3s in: {}", dir.display());
    for f in mp3s_in(dir) {
        println!("• {}", f.file_name().unwrap().to_string_lossy());
        match decode_mp3(&f) {
            Ok(a) => println!("{}", report_line(&measure(&a))),
            Err(e) => println!("  (could not analyze: {e})"),
        }
    }
    println!();
    println!("Run 'acxorcist convert' to write ACX-compliant copies into \"ACX Compliant/\".");
    Ok(())
}

fn run_convert() -> Result<()> {
    let dir = cwd();
    let out_dir = dir.join("ACX Compliant");
    std::fs::create_dir_all(&out_dir)?;
    println!("Converting MP3s in: {}", dir.display());
    println!("Output -> {}", out_dir.display());
    println!();
    let mut count = 0;
    for f in mp3s_in(&dir) {
        let base = f.file_name().unwrap();
        let out = out_dir.join(base);
        println!("▶ {}", base.to_string_lossy());
        let m = convert_file(&f, &out)?;
        println!("{}", report_line(&m));
        println!();
        count += 1;
    }
    println!("Done. Converted {count} file(s) into: {}", out_dir.display());
    println!("Run 'acxorcist verify' to re-check the results.");
    Ok(())
}
