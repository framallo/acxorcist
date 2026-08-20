#!/usr/bin/env bash
#
# End-to-end test for acxorcist. Requires ffmpeg (the default engine) to
# synthesize a test tone; exercises report -> convert -> verify and the engine
# selection / capability gate. Run: ./test.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ACX="$HERE/acxorcist.sh"
pass=0; fail=0
ok()   { printf '\033[1;32mPASS\033[0m %s\n' "$1"; pass=$((pass+1)); }
bad()  { printf '\033[1;31mFAIL\033[0m %s\n' "$1"; fail=$((fail+1)); }

command -v ffmpeg >/dev/null 2>&1 || { echo "SKIP: ffmpeg not installed"; exit 0; }

work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
cd "$work"

# A quiet 3s tone: RMS well below the ACX window so convert has to raise it.
ffmpeg -hide_banner -loglevel error -f lavfi -i "sine=frequency=440:duration=3" \
  -filter:a "volume=-30dB" -c:a libmp3lame -b:a 192k -ar 44100 -ac 2 tone.mp3

# 1. report runs and analyzes the file (default engine, no flags).
# (capture first; `... | grep -q` under pipefail would SIGPIPE the producer)
report_out="$("$ACX" report)"
if printf '%s' "$report_out" | grep -q "RMS"; then ok "report analyzes with default engine"; else bad "report"; fi

# 2. convert produces the output file.
"$ACX" convert >/dev/null
[ -f "ACX Compliant/tone.mp3" ] && ok "convert wrote output" || bad "convert output missing"

# 3. verify: the converted file's RMS lands inside the ACX window (-23..-18).
rms="$("$ACX" verify | sed -n 's/.*RMS *\(-*[0-9.]*\) dB.*/\1/p' | head -1)"
if awk -v v="$rms" 'BEGIN{exit !(v>=-23 && v<=-18)}'; then
  ok "converted RMS $rms dB is inside -23..-18"
else
  bad "converted RMS $rms dB outside window"
fi

# 4. converted peak is at or below -3 dB.
peak="$("$ACX" verify | sed -n 's/.*Peak *\(-*[0-9.]*\) dB.*/\1/p' | head -1)"
if awk -v v="$peak" 'BEGIN{exit !(v<=-3)}'; then ok "converted peak $peak dB <= -3"; else bad "peak $peak dB"; fi

# 5. `engines` reports ffmpeg as fully capable.
engines_out="$("$ACX" engines)"
printf '%s' "$engines_out" | grep -q "capabilities: OK" && ok "engines: ffmpeg is capable" || bad "engines capability"

# 6. a missing engine fails loudly (does not silently fall back).
if "$ACX" --engine acx-nonexistent-bin report >/dev/null 2>&1; then
  bad "missing engine should fail"
else
  ok "missing engine fails loudly"
fi

# 7. capability gate: an engine that exists but lacks the filters cannot convert.
if "$ACX" --engine true convert >/dev/null 2>&1; then
  bad "incapable engine should be blocked from convert"
else
  ok "incapable engine blocked from convert"
fi

echo
echo "Result: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
