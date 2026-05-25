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

Requires [`ffmpeg`](https://ffmpeg.org/) (which includes `ffprobe`):

```bash
brew install ffmpeg          # macOS
sudo apt install ffmpeg      # Debian / Ubuntu
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
chmod +x acx_convert.sh
```

Or just download `acx_convert.sh` and `chmod +x` it.

## Usage

Run inside the folder containing your MP3s. It acts on the current directory. If you used
the one-line installer the command is `acxorcist`; with a manual clone it's
`./acx_convert.sh` — they're the same script.

```bash
acxorcist          # report current ACX compliance of the MP3s (default, no changes)
acxorcist convert  # write ACX-compliant copies -> "ACX Compliant/"
acxorcist verify   # re-check files already in "ACX Compliant/"
```

The default does nothing destructive — it just audits. `convert` never modifies your
originals; compliant copies are written to a new `ACX Compliant/` subfolder with identical
filenames. Each file prints a compliance line:

```
▶ 06 Preface ES.mp3
  RMS  -20.7 dB [OK   | -23..-18]   Peak   -3.6 dB [OK   | <=-3]   Len    414s [OK | <=7200]
```

## Configuration

Tweak the variables at the top of `acx_convert.sh`:

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
