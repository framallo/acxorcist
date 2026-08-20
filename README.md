# acxorcist

Exorcise your MP3s into **ACX-compliant** audiobook files with one command.

[ACX](https://www.acx.com/) (Audiobook Creation Exchange, Audible/Amazon's audiobook
platform) has strict technical requirements for every uploaded audio file. `acxorcist`
batch-converts every MP3 in a folder so it meets those requirements — measuring each
file, normalizing loudness, taming peaks, and adding room tone.

It is a **single self-contained binary**. MP3 decoding, loudness/peak analysis, the
gain / limiter / room-tone mastering chain, resampling, and CBR MP3 encoding all happen
in-process (pure-Rust [`symphonia`](https://github.com/pdeljanov/Symphonia) decoder +
`libmp3lame` compiled in). **No `ffmpeg` or any other external tool is required.**

## What it enforces

| Spec | ACX requirement | What acxorcist does |
|---|---|---|
| RMS loudness | between **-23 dB and -18 dB** | gain each file to ~**-20 dB** (window center) |
| Peak | no higher than **-3 dB** | look-ahead limiter holds peaks at ~**-3.5 dB** |
| Format | MP3, **192 kbps CBR** or higher | encodes 192 kbps **CBR** |
| Sample rate | **44.1 kHz** | resamples to 44.1 kHz |
| Channels | consistent throughout | forces stereo |
| Room tone | silence at head & tail | adds **0.5 s** lead + **2 s** tail |
| Length | each file **≤ 120 min** | reported per file |

> **Note:** ACX also requires a noise floor below **-60 dB RMS**. acxorcist can't reliably
> measure that, so it's not auto-checked — give noisy recordings a listen, or run a file
> through Audacity's free *ACX Check* plugin as a final gut-check.

## Why direct RMS targeting (not `loudnorm`)

EBU R128 `loudnorm` targets gated **LUFS**, which reads several dB louder than the
whole-file **RMS** that ACX actually measures — especially on spoken-word audio with lots
of natural pauses. A fixed LUFS target can land files *below* the -23 dB floor. acxorcist
instead measures each file's RMS and applies the exact gain needed, then limits peaks — so
results land in the window predictably, regardless of how much silence a file contains.

## Install

Homebrew (macOS / Linux):

```sh
brew install framallo/acxorcist/acxorcist
```

Prebuilt binary via `curl | sh` (macOS / Linux):

```sh
curl -fsSL https://raw.githubusercontent.com/framallo/acxorcist/main/install.sh | sh
```

With Cargo (any platform with a Rust toolchain and a C compiler for libmp3lame):

```sh
cargo install --git https://github.com/framallo/acxorcist
```

## Usage

Run inside the folder containing your MP3s. It acts on the current directory.

```sh
acxorcist          # report current ACX compliance of the MP3s (default, no changes)
acxorcist convert  # write ACX-compliant copies -> "ACX Compliant/"
acxorcist verify   # re-check files already in "ACX Compliant/"
```

The default does nothing destructive — it just audits. `convert` never modifies your
originals; compliant copies are written to a new `ACX Compliant/` subfolder with identical
filenames (ID3 tags are carried over). Each file prints a compliance line:

```
▶ introduction.mp3
  RMS  -20.7 dB [OK   | -23..-18]   Peak   -3.6 dB [OK   | <=-3]   Len    414s [OK | <=7200]
```

## Use as a Rust library

```rust
use std::path::Path;
use acxorcist::{convert_file, decode_mp3, measure};

let m = measure(&decode_mp3(Path::new("chapter.mp3"))?);
println!("RMS {:.1} dB, peak {:.1} dB", m.mean_db, m.peak_db);

convert_file(Path::new("chapter.mp3"), Path::new("out.mp3"))?; // ACX-compliant output
# Ok::<(), anyhow::Error>(())
```

## Configuration

The ACX targets are constants at the top of `src/audio.rs` (`TARGET_RMS_DB`,
`PEAK_LIMIT`, `LEAD_SILENCE_S`, `TAIL_SILENCE_S`, `OUT_SAMPLE_RATE`, `OUT_BITRATE_KBPS`).

## Tests

```sh
cargo test
```

- `dsp` — unit tests for the measure / gain / limiter / room-tone blocks.
- `referee` — end-to-end: acxorcist converts a file with **zero ffmpeg involvement**,
  then ffmpeg's `volumedetect` (an independent referee) confirms the output lands in the
  ACX window with peaks ≤ -3 dB, and `ffprobe` confirms 192 kbps CBR / 44.1 kHz / stereo.
  Skips when ffmpeg is absent.

## History

The original acxorcist was a Bash script that shelled out to `ffmpeg`. It is preserved
under [`legacy/`](legacy/) for reference; the current tool is the self-contained Rust
binary described above.

## License

MIT
