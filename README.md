# Samsung Q900T synchronized Linux audio

An experimental Rust bridge that routes Linux system audio to a PipeWire virtual sink, mirrors it to a local output, and sends uncompressed multichannel PCM over RTP/Wi-Fi. The sender supports 5.1 (6 channels) and 7.1 (8 channels), using 24-bit/48 kHz PCM. This avoids Bluetooth audio codecs such as SBC on the bridge path.

The destination and open product decisions are tracked in the [Wayfinder map](https://github.com/guility/samsung-q900-wifi-linux/issues/1). Implementation work is tracked in [issue #10](https://github.com/guility/samsung-q900-wifi-linux/issues/10); direct Q900T HDMI LPCM acceptance is pending in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12).

## Requirements

- Linux with PipeWire, its PulseAudio compatibility server (`pactl`), and the `pw-loopback` client.
- IPv4 multicast enabled on the Wi-Fi LAN. The default RTP/SAP address is `224.0.0.56`, with RTP port `5004`.
- The receiver must expose an active PipeWire sink with the requested channel count. Standard HDMI profiles must report the expected speaker positions. For a Pro Audio sink with `AUX0…AUX7` positions, the matching HDMI card port must also report connected; a Pro Audio profile alone does not prove an HDMI device is present.
- To connect the receiver to a Q900T, the HDMI source and Q900T must negotiate multichannel LPCM. This has not yet been verified on the available hardware. The Q900T Bluetooth A2DP path is stereo SBC and is rejected as a local mirror by the sender.

## Build and inspect

```sh
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

Preview a sender setup without changing audio routing or sending network traffic:

```sh
cargo run -- sender \
  --layout 7.1 \
  --destination-ip 224.0.0.56 \
  --local-sink alsa_output.YOUR_CARD.HiFi__Speaker__sink \
  --dry-run
```

Run an end-to-end software loopback through PipeWire's RTP modules without a soundbar:

```sh
bash acceptance/loopback.sh 5.1
bash acceptance/loopback.sh 7.1
```

The loopback script creates temporary virtual sinks, sends a distinct tone on every channel, records the receiver sink, checks that each channel contains audio, and cleans up the sinks and routes.

## Analyze acoustic synchronization measurements

The `sync-report` command analyzes repeated acoustic arrival measurements from a CSV file:

```sh
cargo run -- sync-report --input acceptance/sample-sync-observations.csv
```

The required columns are `elapsed_s,local_ms,q900_ms`. Both arrival columns must use the same recorder timebase; use a single microphone/recorder clock at the listening position. The example file is synthetic and demonstrates an 8 ms fixed offset only. The report fits fixed offset and linear drift, summarizes absolute skew and residual jitter, and recommends which output to delay at the start of the sample.

Apply the report's fixed-delay recommendation when starting playback: use `--local-delay-ms N` on the sender when the local output arrives first, or `--output-delay-ms N` on the receiver when the Q900T output arrives first. Both use PipeWire's explicit `pw-loopback --delay` control and accept 0–2000 ms. They compensate a fixed offset only; drift needs a shared clock and remains uncorrected. The `--latency-ms` setting controls buffering, not added delay.

The report does not record the microphone or alter playback timing itself. A measured output offset is not a video or game input-latency result. Use the separate acoustic, video, and game procedures in [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2). The current 100 ms RTP settings remain buffers, not synchronization guarantees.

Start the sender on the source host:

```sh
cargo run --release -- sender \
  --layout 5.1 \
  --destination-ip 224.0.0.56 \
  --local-sink alsa_output.YOUR_CARD.HiFi__Speaker__sink
```

Start a receiver on a Linux host connected to the Q900T by HDMI:

```sh
cargo run --release -- receiver \
  --layout 5.1 \
  --sink alsa_output.YOUR_HDMI_CARD.HiFi__Pro__sink
```

Use the same layout at both ends. The sender temporarily sets its multichannel virtual sink as the system default, moves existing playback streams to it, mirrors the audio to `--local-sink`, and sends raw PCM over RTP. Press Ctrl-C to restore the original default and stream routes. The receiver does not change the receiver host's default sink. Both commands default to a 100 ms latency setting; this is a starting buffer value, not a synchronization guarantee.

The receiver checks the active PipeWire channel count and channel positions before loading modules. It receives RTP into an internal sink with the stream's channel order, then maps channel positions to the selected HDMI sink. For Pro Audio sinks with raw AUX positions, it creates a remap sink with the standard HDMI channel map and refuses to start unless PipeWire reports the corresponding HDMI port connected. This preserves the side/rear channel distinction in 7.1 without treating an unconnected Pro Audio device as hardware acceptance. PipeWire provides explicit master and output channel maps for this remap ([module-remap-sink](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_remap_sink.html)). Run `--dry-run` first to inspect a configuration.

## Current verification limits

The Rust acceptance tests check 5.1/7.1 channel maps, RTP module plans, PipeWire sink parsing, Pro Audio HDMI-port availability, rejection of incompatible outputs, and analysis of acoustic sync observations. The host-only RTP acceptance script sends a distinct tone on every channel through a simulated AUX sink and checks channel identity after the receiver mapping. On this host, the NVIDIA Pro Audio sinks expose eight AUX channels but all HDMI ELD entries report no monitor, so the receiver rejects them for hardware playback. The Q900T itself is visible only as a two-channel Bluetooth sink; cross-host Wi-Fi, HDMI LPCM to the soundbar, and acoustic synchronization remain unverified in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12) and [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

Acceptance scenarios are recorded in [`acceptance/multichannel.feature`](acceptance/multichannel.feature). Research reports are stored under `research/` on short-lived `research/<topic>` branches and linked from the relevant decision issues.
