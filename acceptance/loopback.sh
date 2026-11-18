#!/usr/bin/env bash
set -euo pipefail

layout="${1:-5.1}"
case "$layout" in
  5.1)
    channels=6
    channel_map="5.1"
    pa_channel_map="front-left,front-right,front-center,lfe,rear-left,rear-right"
    receive_port="receive_AUX6"
    ;;
  7.1)
    channels=8
    channel_map="7.1"
    pa_channel_map="front-left,front-right,front-center,lfe,rear-left,rear-right,side-left,side-right"
    receive_port="receive_AUX8"
    ;;
  *)
    echo "usage: $0 [5.1|7.1]" >&2
    exit 2
    ;;
esac

for tool in cargo pactl pw-cat pw-link python3; do
  command -v "$tool" >/dev/null || { echo "missing required command: $tool" >&2; exit 1; }
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work_dir="$(mktemp -d)"
suffix="$$"
rx_sink="q900_accept_rx_${suffix}"
local_sink="q900_accept_local_${suffix}"
source_file="$work_dir/source.pcm"
capture_file="$work_dir/capture.pcm"
rx_module=""
local_module=""
receiver_pid=""
sender_pid=""
record_pid=""

cleanup() {
  set +e
  for pid in "$record_pid" "$sender_pid" "$receiver_pid"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -INT "$pid" 2>/dev/null
    fi
  done
  for pid in "$record_pid" "$sender_pid" "$receiver_pid"; do
    if [[ -n "$pid" ]]; then
      wait "$pid" 2>/dev/null
    fi
  done
  if [[ -n "$local_module" ]]; then pactl unload-module "$local_module" >/dev/null 2>&1; fi
  if [[ -n "$rx_module" ]]; then pactl unload-module "$rx_module" >/dev/null 2>&1; fi
  python3 - "$work_dir" <<'PY'
from pathlib import Path
import shutil
import sys

shutil.rmtree(sys.argv[1], ignore_errors=True)
PY
}
trap cleanup EXIT
trap 'exit 130' INT TERM

cargo build --manifest-path "$repo_root/Cargo.toml" --quiet
rx_module="$(pactl load-module module-null-sink \
  "sink_name=$rx_sink" format=s24be "channels=$channels" rate=48000 \
  "channel_map=$pa_channel_map" "sink_properties=device.description=Q900_Acceptance_Receiver")"
local_module="$(pactl load-module module-null-sink \
  "sink_name=$local_sink" format=s24be channels=2 rate=48000 \
  channel_map=front-left,front-right sink_properties=device.description=Q900_Acceptance_Local)"

"$repo_root/target/debug/q900-bridge" receiver \
  --sink "$rx_sink" --layout "$layout" --latency-ms 100 \
  >"$work_dir/receiver.log" 2>&1 &
receiver_pid=$!
"$repo_root/target/debug/q900-bridge" sender \
  --layout "$layout" --destination-ip 224.0.0.56 \
  --local-sink "$local_sink" --latency-ms 100 \
  >"$work_dir/sender.log" 2>&1 &
sender_pid=$!
bus_sink="q900_bridge_${sender_pid}"

connected=false
for ((attempt = 0; attempt < 50; attempt++)); do
  if ! kill -0 "$receiver_pid" 2>/dev/null || ! kill -0 "$sender_pid" 2>/dev/null; then
    cat "$work_dir/receiver.log" "$work_dir/sender.log" >&2
    exit 1
  fi
  if pw-link -l | grep -F "$rx_sink:playback_FL" >/dev/null \
      && pw-link -l | grep -F "Q900_Bridge:$receive_port" >/dev/null; then
    connected=true
    break
  fi
  sleep 0.2
done
if [[ "$connected" != true ]]; then
  echo "RTP receiver did not connect to the $layout test sink" >&2
  cat "$work_dir/receiver.log" "$work_dir/sender.log" >&2
  exit 1
fi

python3 - "$source_file" "$channels" <<'PY'
import math
import sys

path, channels = sys.argv[1], int(sys.argv[2])
rate, seconds = 48_000, 3
frequencies = [440 + channel * 137 for channel in range(channels)]
amplitude = 0.08 * ((1 << 23) - 1)
with open(path, "wb") as output:
    for frame in range(rate * seconds):
        for frequency in frequencies:
            sample = round(amplitude * math.sin(2 * math.pi * frequency * frame / rate)) << 8
            output.write(sample.to_bytes(4, "little", signed=True))
PY

monitor_id="$(pactl list short sources | awk -v name="$rx_sink.monitor" '$2 == name {print $1}')"
if [[ -z "$monitor_id" ]]; then
  echo "could not find the receiver monitor source" >&2
  exit 1
fi

pw-cat --record --raw --target "$monitor_id" --rate 48000 \
  --channels "$channels" --channel-map "$channel_map" --format s32 \
  "$capture_file" >"$work_dir/record.log" 2>&1 &
record_pid=$!
sleep 0.3
pw-cat --playback --raw --target "$bus_sink" --rate 48000 \
  --channels "$channels" --channel-map "$channel_map" --format s32 \
  --volume 0.1 "$source_file"
sleep 0.5
kill -INT "$record_pid"
wait "$record_pid" || true
record_pid=""

python3 - "$capture_file" "$channels" <<'PY'
import array
import math
import sys

path, channels = sys.argv[1], int(sys.argv[2])
samples = array.array("i")
with open(path, "rb") as capture:
    samples.frombytes(capture.read())
if samples.itemsize != 4 or len(samples) < 48_000 * channels:
    raise SystemExit("receiver capture was too short to verify channel audio")
frames = len(samples) // channels
rms = [
    math.sqrt(sum(samples[frame * channels + channel] ** 2 for frame in range(frames)) / frames)
    for channel in range(channels)
]
if any(level < 100_000 for level in rms):
    raise SystemExit(f"one or more RTP channels were silent: {[round(x) for x in rms]}")
print(f"RTP {channels}-channel loopback received audio on every channel; RMS={[round(x) for x in rms]}")
PY
