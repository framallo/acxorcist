#!/usr/bin/env bash
#
# acxorcist — Convert MP3 files in this folder to ACX-compliant audio.
#
# ACX submission requirements enforced here:
#   - RMS loudness between -23 dB and -18 dB   (we target ~ -20 dB)
#   - Peak no higher than -3 dB                (we limit at -3.5 dBTP)
#   - Noise floor below -60 dB RMS             (padding is digital silence)
#   - MP3, 192 kbps CBR or higher              (we encode 192 kbps CBR)
#   - 44.1 kHz sample rate
#   - Consistent channels (stereo)
#   - Room tone: ~0.5 s silence at head, ~2 s at tail
#   - Each file under 120 minutes
#
# Usage (runs on the current working directory):
#   acxorcist            Report current ACX compliance of the MP3s (no changes)
#   acxorcist convert    Convert every *.mp3 here -> "ACX Compliant/"
#   acxorcist verify     Report compliance of files already in "ACX Compliant/"
#   acxorcist engines    Show the selected engine and its capabilities
#
# Engine selection (see "Engines" below):
#   --engine <bin>       Use an ffmpeg-CLI-compatible binary (default: ffmpeg)
#   ACXORCIST_ENGINE=... Same, via environment.

set -euo pipefail

# --- Configuration -----------------------------------------------------------
IN_DIR="$PWD"
OUT_DIR="$PWD/ACX Compliant"

TARGET_RMS="-20"    # target RMS (dB) -> center of ACX -23..-18 window
PEAK_LIMIT="0.668"  # alimiter ceiling, linear: 0.668 ~= -3.5 dB (keeps peak <= -3)
LEAD_SILENCE="0.5"  # seconds of room tone at the head
TAIL_SILENCE="2.0"  # seconds of room tone at the tail
BITRATE="192k"      # CBR MP3 bitrate
SAMPLE_RATE="44100"

# =============================================================================
# Engines
# -----------------------------------------------------------------------------
# acxorcist does not call ffmpeg directly. It drives an *engine*: any binary
# that speaks ffmpeg's command-line interface. `ffmpeg` is the default and the
# only engine known to implement the full audio pipeline today, but the goal is
# to run with a pure-Rust, dependency-free engine as those mature. Point
# acxorcist at any compatible binary with `--engine <bin>` (or ACXORCIST_ENGINE);
# it is capability-checked before use, so an incapable engine fails loudly
# instead of producing non-compliant audio. See README "Engines".
# =============================================================================
ENGINE="${ACXORCIST_ENGINE:-ffmpeg}"

have() { command -v "$1" >/dev/null 2>&1; }
die()  { echo "ERROR: $*" >&2; exit 1; }

# The audio filters and encoder the convert pipeline needs from an engine.
REQUIRED_FILTERS=(volume alimiter adelay apad aresample volumedetect)
REQUIRED_ENCODER="libmp3lame"

# engine_present: the engine binary exists on PATH.
engine_present() { have "$ENGINE"; }

# engine_analyze <file> -> echoes "MEAN|MAX|DURATION" (dB, dB, seconds).
# Parses one ffmpeg-compatible `-i ... volumedetect` run: mean/max volume from
# the volumedetect summary, duration from the container header (`Duration:`),
# so no separate ffprobe call is needed and any compatible engine works.
engine_analyze() {
  local f="$1" out mean max dur
  out="$("$ENGINE" -hide_banner -i "$f" -af volumedetect -f null /dev/null 2>&1 || true)"
  mean="$(printf '%s\n' "$out" | sed -n 's/.*mean_volume: \(-*[0-9.]*\) dB.*/\1/p' | tail -1)"
  max="$(printf  '%s\n' "$out" | sed -n 's/.*max_volume: \(-*[0-9.]*\) dB.*/\1/p'  | tail -1)"
  # "  Duration: 00:00:03.09, start: ..." -> seconds
  dur="$(printf '%s\n' "$out" \
    | sed -n 's/.*Duration: \([0-9][0-9]\):\([0-9][0-9]\):\([0-9][0-9.]*\).*/\1 \2 \3/p' | head -1 \
    | awk '{printf "%.2f", $1*3600 + $2*60 + $3}')"
  printf '%s|%s|%s' "$mean" "$max" "$dur"
}

# engine_convert <in> <out>: gain to TARGET_RMS, limit peaks, add room tone,
# resample, and encode CBR MP3 — all via the selected engine.
engine_convert() {
  local in="$1" out="$2" mean gain lead_ms filter
  mean="$(engine_analyze "$in" | cut -d'|' -f1)"
  [ -n "$mean" ] || die "engine '$ENGINE' could not measure loudness of $(basename "$in")"
  gain="$(awk -v m="$mean" -v t="$TARGET_RMS" 'BEGIN{printf "%.2f", t - m}')"
  lead_ms="$(awk -v s="$LEAD_SILENCE" 'BEGIN{printf "%d", s*1000}')"
  filter="volume=${gain}dB,alimiter=limit=${PEAK_LIMIT}:level=false,adelay=${lead_ms}|${lead_ms},apad=pad_dur=${TAIL_SILENCE},aresample=${SAMPLE_RATE}:first_pts=0"
  "$ENGINE" -hide_banner -loglevel error -y -i "$in" \
    -af "$filter" \
    -map_metadata 0 \
    -c:a "$REQUIRED_ENCODER" -b:a "$BITRATE" -ar "$SAMPLE_RATE" -ac 2 \
    "$out"
}

# engine_missing_caps: echoes a space-separated list of missing capabilities
# ("filter:alimiter", "encoder:libmp3lame", …); empty output means fully capable.
engine_missing_caps() {
  local filters encoders missing="" flt
  filters="$("$ENGINE" -hide_banner -filters 2>/dev/null || true)"
  encoders="$("$ENGINE" -hide_banner -encoders 2>/dev/null || true)"
  for flt in "${REQUIRED_FILTERS[@]}"; do
    printf '%s\n' "$filters" | grep -qw "$flt" || missing="$missing filter:$flt"
  done
  printf '%s\n' "$encoders" | grep -qw "$REQUIRED_ENCODER" || missing="$missing encoder:$REQUIRED_ENCODER"
  printf '%s' "${missing# }"
}

# Gate used before any analysis (report/verify/convert): the binary must exist.
require_engine() {
  engine_present || die "engine '$ENGINE' not found on PATH.
  Default engine is ffmpeg (brew install ffmpeg / apt install ffmpeg).
  Or select a compatible engine: --engine <bin>  (or ACXORCIST_ENGINE=<bin>)."
}

# Stronger gate used before convert: the engine must expose the full pipeline.
require_engine_convert() {
  require_engine
  local missing; missing="$(engine_missing_caps)"
  if [ -n "$missing" ]; then
    die "engine '$ENGINE' cannot run the ACX pipeline; missing: $missing
  ffmpeg (the default) implements all of these. Alternative engines must expose
  the same filters and a CBR MP3 encoder before they can convert."
  fi
}

# --- Report helper -----------------------------------------------------------
# Print mean/max volume + duration, and flag against ACX thresholds.
report_file() {
  local f="$1" a mean max dur rms_ok peak_ok len_ok
  a="$(engine_analyze "$f")"
  mean="$(printf '%s' "$a" | cut -d'|' -f1)"
  max="$(printf  '%s' "$a" | cut -d'|' -f2)"
  dur="$(printf  '%s' "$a" | cut -d'|' -f3)"
  [ -n "$mean" ] || { printf '  (could not analyze with engine "%s")\n' "$ENGINE"; return; }

  rms_ok=$(awk -v v="$mean" 'BEGIN{print (v>=-23 && v<=-18)?"OK":"FAIL"}')
  peak_ok=$(awk -v v="$max"  'BEGIN{print (v<=-3)?"OK":"FAIL"}')
  len_ok=$(awk -v v="$dur"  'BEGIN{print (v<=7200)?"OK":"FAIL"}')
  printf '  RMS %6s dB [%-4s | -23..-18]   Peak %6s dB [%-4s | <=-3]   Len %6.0fs [%s | <=7200]\n' \
    "$mean" "$rms_ok" "$max" "$peak_ok" "$dur" "$len_ok"
}

usage() {
  sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'
}

# --- Argument parsing --------------------------------------------------------
MODE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --engine)   [ $# -ge 2 ] || die "--engine needs a binary name"; ENGINE="$2"; shift 2 ;;
    --engine=*) ENGINE="${1#*=}"; shift ;;
    -h|--help)  usage; exit 0 ;;
    report|check|convert|verify|engines) MODE="$1"; shift ;;
    *) echo "Unknown argument: $1" >&2; echo "Usage: acxorcist [report|convert|verify|engines] [--engine <bin>]" >&2; exit 2 ;;
  esac
done
MODE="${MODE:-report}"

# --- engines: show selection + capabilities ----------------------------------
if [[ "$MODE" == "engines" ]]; then
  echo "Selected engine: $ENGINE"
  if ! engine_present; then
    echo "  status: NOT FOUND on PATH"
    echo "  install ffmpeg (default) or pass --engine <compatible-bin>"
    exit 1
  fi
  echo "  status: found ($(command -v "$ENGINE"))"
  missing="$(engine_missing_caps)"
  if [ -z "$missing" ]; then
    echo "  capabilities: OK (report + convert supported)"
  else
    echo "  capabilities: INCOMPLETE — cannot convert"
    echo "  missing: $missing"
  fi
  echo
  echo "Override with:  --engine <bin>   or   ACXORCIST_ENGINE=<bin>"
  exit 0
fi

# --- Modes -------------------------------------------------------------------
if [[ "$MODE" == "report" || "$MODE" == "check" ]]; then
  require_engine
  echo "ACX compliance report for MP3s in: $IN_DIR   (engine: $ENGINE)"
  shopt -s nullglob
  for f in "$IN_DIR"/*.mp3; do
    echo "• $(basename "$f")"
    report_file "$f"
  done
  echo
  echo "Run 'acxorcist convert' to write ACX-compliant copies into \"ACX Compliant/\"."
  exit 0
fi

if [[ "$MODE" == "verify" ]]; then
  require_engine
  echo "Verifying converted files in: $OUT_DIR   (engine: $ENGINE)"
  shopt -s nullglob
  for f in "$OUT_DIR"/*.mp3; do
    echo "• $(basename "$f")"
    report_file "$f"
  done
  exit 0
fi

# --- Convert -----------------------------------------------------------------
require_engine_convert
mkdir -p "$OUT_DIR"
echo "Converting MP3s in: $IN_DIR   (engine: $ENGINE)"
echo "Output -> $OUT_DIR"
echo

shopt -s nullglob
count=0
for f in "$IN_DIR"/*.mp3; do
  base="$(basename "$f")"
  out="$OUT_DIR/$base"
  echo "▶ $base"
  engine_convert "$f" "$out"
  report_file "$out"
  count=$((count+1))
  echo
done

echo "Done. Converted $count file(s) into: $OUT_DIR"
echo "Run 'acxorcist verify' to re-check the results."
