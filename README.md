# Samsung Q900T synchronized Linux audio

An experimental Rust bridge that routes Linux system audio to a PipeWire virtual sink, mirrors it to a local output, and sends uncompressed multichannel PCM over RTP/Wi-Fi. The sender supports 5.1 (6 channels) and 7.1 (8 channels), using 24-bit/48 kHz PCM. This avoids Bluetooth audio codecs such as SBC on the bridge path.

The destination and open product decisions are tracked in the [Wayfinder map](https://github.com/guility/samsung-q900-wifi-linux/issues/1). Implementation work is tracked in [issue #10](https://github.com/guility/samsung-q900-wifi-linux/issues/10); hardware acceptance for Q900T LPCM is still pending in [issue #9](https://github.com/guility/samsung-q900-wifi-linux/issues/9).

## Requirements

- Linux with PipeWire and its PulseAudio compatibility server (`pactl`).
- IPv4 multicast enabled on the Wi-Fi LAN. The default RTP/SAP address is `224.0.0.56`, with RTP port `5004`.
- The receiver must expose an active PipeWire sink with the requested channel count and positions. Select the HDMI card's **Pro Audio** profile when available, then verify it with `pw-dump`.
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

The receiver checks the active PipeWire channel count and channel positions before loading modules. It receives RTP into an internal sink with the stream's channel order, then uses an eight-channel-aware PipeWire loopback to map channel positions to the selected HDMI sink. This preserves the side/rear channel distinction in 7.1. It refuses stereo or mismatched sinks instead of silently accepting a downmix. Run `--dry-run` first to inspect a configuration.

## Current verification limits

The Rust acceptance tests check 5.1/7.1 channel maps, RTP module plans, PipeWire sink parsing, and rejection of incompatible output sinks. The host-only RTP acceptance script sends a distinct tone on every channel and checks channel identity after the receiver mapping. This verifies the software audio graph, not the soundbar hardware. The currently attached Samsung Q900T is visible only as a two-channel Bluetooth sink; it cannot validate this Wi-Fi/HDMI LPCM route. No claim of Q900T 5.1/7.1 support or synchronized playback is made until the hardware checks in issue #9 pass.

Acceptance scenarios are recorded in [`acceptance/multichannel.feature`](acceptance/multichannel.feature). Research reports are stored under `research/` on short-lived `research/<topic>` branches and linked from the relevant decision issues.
