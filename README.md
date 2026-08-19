# acxorcist

Exorcise your MP3s into **ACX-compliant** audiobook files with one command.

[ACX](https://www.acx.com/) (Audiobook Creation Exchange, Audible/Amazon's audiobook
platform) has strict technical requirements for every uploaded audio file. `acxorcist`
is a small, dependency-light Bash script that batch-converts every MP3 in a folder so it
meets those requirements — measuring each file, normalizing loudness, taming peaks, and
adding room tone.

## What it enforces

| Spec | ACX requirement | What the script does |
|---|---|---|
| RMS loudness | between **-23 dB and -18 dB** | gain each file to ~**-20 dB** (window center) |
| Peak | no higher than **-3 dB** | hard-limits peaks at ~**-3.5 dB** |
| Format | MP3, **192 kbps CBR** or higher | encodes 192 kbps **CBR** |
| Sample rate | **44.1 kHz** | resamples to 44.1 kHz |
| Channels | consistent throughout | forces stereo |
| Room tone | silence at head & tail | adds **0.5 s** lead + **2 s** tail |
| Length | each file **≤ 120 min** | reported per file |

> **Note:** ACX also requires a noise floor below **-60 dB RMS**. The script can't reliably
> measure that, so it's not auto-checked — give noisy recordings a listen, or run a file
> through Audacity's free *ACX Check* plugin as a final gut-check.

## Why direct RMS targeting (not `loudnorm`)

EBU R128 `loudnorm` targets gated **LUFS**, which reads several dB louder than the
whole-file **RMS** that ACX actually measures — especially on spoken-word audio with lots
of natural pauses. A fixed LUFS target can land files *below* the -23 dB floor. `acxorcist`
instead measures each file's RMS and applies the exact gain needed, then limits peaks — the
same chain as Audacity's ACX mastering macro — so results land in the window predictably,
regardless of how much silence a file contains.

## Install

Needs an **engine** — an ffmpeg-CLI-compatible binary that does the audio work.
[`ffmpeg`](https://ffmpeg.org/) is the default and the only one that runs the full
pipeline today (see [Engines](#engines)):

```bash
brew install ffmpeg          # macOS
sudo apt install ffmpeg      # Debian / Ubuntu
```

Homebrew (installs the script and pulls in ffmpeg):

```bash
brew install framallo/acxorcist/acxorcist
```

### One-line install

Installs an `acxorcist` command onto your PATH:

```bash
curl -fsSL https://raw.githubusercontent.com/framallo/acxorcist/main/install.sh | bash
```

### Manual install

```bash
git clone https://github.com/framallo/acxorcist.git
cd acxorcist
chmod +x acxorcist
```

Or just download `acxorcist` and `chmod +x` it.

## Usage

Run inside the folder containing your MP3s. It acts on the current directory. The command
is `acxorcist` (or `./acxorcist` if you cloned manually and it isn't on your PATH).

```bash
acxorcist          # report current ACX compliance of the MP3s (default, no changes)
acxorcist convert  # write ACX-compliant copies -> "ACX Compliant/"
acxorcist verify   # re-check files already in "ACX Compliant/"
acxorcist engines  # show the selected engine and whether it can convert
```

Select a different engine for any command:

```bash
acxorcist --engine <bin> convert     # or: ACXORCIST_ENGINE=<bin> acxorcist convert
```

The default does nothing destructive — it just audits. `convert` never modifies your
originals; compliant copies are written to a new `ACX Compliant/` subfolder with identical
filenames. Each file prints a compliance line:

```
▶ introduction.mp3
  RMS  -20.7 dB [OK   | -23..-18]   Peak   -3.6 dB [OK   | <=-3]   Len    414s [OK | <=7200]
```

## Engines

acxorcist never calls `ffmpeg` by name in its logic. It drives an **engine**: any
binary that speaks ffmpeg's command-line interface (`-i`, `-af`, `-c:a`, …).
The engine does every heavy step — measuring loudness, applying the gain /
limiter / room-tone filter chain, and encoding CBR MP3.

Pick one with `--engine <bin>` or the `ACXORCIST_ENGINE` environment variable;
the default is `ffmpeg`. Before converting, acxorcist **capability-checks** the
engine (it must expose the `volume`, `alimiter`, `adelay`, `apad`, `aresample`,
`volumedetect` filters and a `libmp3lame` CBR encoder). An engine that is missing
or incapable fails loudly, naming what's missing — it never silently falls back
or produces non-compliant audio. Run `acxorcist engines` to see the status of
the current selection.

### Goal: a dependency-free, pure-Rust engine

The aim is to run acxorcist with **no binary dependency on ffmpeg**, using a
memory-safe, pure-Rust engine instead. Because the engine interface is just
"an ffmpeg-CLI-compatible binary", any such tool works with **zero changes** to
acxorcist — you only point `--engine` at it.

**Reality today:** ffmpeg remains the only engine that can run the full ACX
*audio* pipeline. The pure-Rust FFmpeg efforts
([`remade_ffmpeg_rs`](https://github.com/Remade-With-Rust/remade_ffmpeg_rs),
[`ffmpreg`](https://github.com/yazaldefilimone/ffmpreg)) are early-stage and
focused on video decode; none yet implements the audio filters and MP3 CBR
encoder acxorcist needs. When one does — or a small purpose-built Rust audio
engine appears — set `--engine` to it and ffmpeg becomes optional. Until then
ffmpeg is the default and recommended engine.

## Configuration

Tweak the variables at the top of `acxorcist`:

| Variable | Default | Meaning |
|---|---|---|
| `TARGET_RMS` | `-20` | target RMS in dB (center of the ACX window) |
| `PEAK_LIMIT` | `0.668` | limiter ceiling, linear (`0.668` ≈ -3.5 dB) |
| `LEAD_SILENCE` | `0.5` | seconds of room tone at the head |
| `TAIL_SILENCE` | `2.0` | seconds of room tone at the tail |
| `BITRATE` | `192k` | CBR MP3 bitrate |
| `SAMPLE_RATE` | `44100` | output sample rate in Hz |

## License

MIT
