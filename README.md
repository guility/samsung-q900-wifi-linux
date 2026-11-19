# Samsung Q900T synchronized Linux audio

An experimental Rust bridge that routes Linux system audio to a PipeWire virtual sink, mirrors it to a local output, and sends uncompressed multichannel PCM over RTP/Wi-Fi. The sender supports 5.1 (6 channels) and 7.1 (8 channels), using 24-bit/48 kHz PCM. This avoids Bluetooth audio codecs such as SBC on the bridge path.

The wireless leg uses raw linear PCM rather than a lossy audio codec, so it preserves the decoded audio samples and discrete channels. Its audio payload is 6.912 Mbit/s for 5.1 and 9.216 Mbit/s for 7.1, before RTP/UDP/IP overhead. The current same-host loopback confirms channel preservation; sustained delivery across two Wi-Fi hosts still needs acceptance testing.

The destination and open product decisions are tracked in the [Wayfinder map](https://github.com/guility/samsung-q900-wifi-linux/issues/1). Implementation work is tracked in [issue #10](https://github.com/guility/samsung-q900-wifi-linux/issues/10); direct Q900T HDMI LPCM acceptance is pending in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12).

## Requirements

- Linux with PipeWire, its PulseAudio compatibility server (`pactl`), and the `pw-loopback` client.
- IPv4 multicast enabled on the Wi-Fi LAN. The default RTP/SAP address is `224.0.0.56`, with RTP port `5004`.
- On hosts with multiple network interfaces, pass `--source-ip` with the IPv4 address assigned to Wi-Fi (for example, find it with `ip -4 addr show dev wlan0`). If omitted, the system routing table chooses the outbound interface. PipeWire's RTP sender supports this source address setting ([module-rtp-send](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_send.html)).
- The receiver must expose an active PipeWire sink with the requested channel count. Standard HDMI profiles must report the expected speaker positions. For a Pro Audio sink with `AUX0…AUX7` positions, the matching HDMI card port must also report connected; a Pro Audio profile alone does not prove an HDMI device is present.
- To connect the receiver to a Q900T, the HDMI source and Q900T must negotiate multichannel LPCM. This has not yet been verified on the available hardware. The host currently exposes the Q900T over two-channel A2DP with `SBC` and `SBC-XQ` profiles. Neither supports the required 5.1/7.1 path, so the bridge rejects this Bluetooth sink; direct HDMI LPCM to the Q900T remains unverified.

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
  --source-ip 192.168.1.57 \
  --local-sink alsa_output.YOUR_CARD.HiFi__Speaker__sink \
  --dry-run
```

Replace `192.168.1.57` with the IPv4 address assigned to the Wi-Fi interface. `--source-ip` binds the RTP sender to that local address; without it, the operating system selects the route.

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

Apply the report's fixed-delay recommendation when starting playback: use `--local-delay-ms N` on the sender when the local output arrives first, or `--output-delay-ms N` on the receiver when the Q900T output arrives first. Both use PipeWire's explicit `pw-loopback --delay` control and accept 0–2000 ms. They compensate a fixed acoustic offset; they do not set the initial synchronization point automatically. The receiver uses PipeWire RTP's default constant-latency mode, which adjusts buffer consumption to track sender/receiver clock-rate differences. That mechanism does not guarantee acoustic alignment, so skew and drift still need measurement. The `--latency-ms` setting controls the RTP buffer target, not added delay or a synchronization guarantee ([PipeWire RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html), [RTP internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html)).

The report does not record the microphone or alter playback timing itself. A measured output offset is not a video or game input-latency result. Use the separate acoustic, video, and game procedures in [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2). The current 100 ms RTP settings remain buffers, not synchronization guarantees.

Start the sender on the source host:

```sh
cargo run --release -- sender \
  --layout 5.1 \
  --destination-ip 224.0.0.56 \
  --source-ip 192.168.1.57 \
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

## Check HDMI LPCM channel mapping

After connecting the Linux HDMI output directly to Q900T HDMI IN 1 or 2, select the matching HDMI input on the soundbar and run the channel check against the active HDMI sink:

```sh
cargo run -- lpcm-check --sink alsa_output.YOUR_HDMI_CARD.HiFi__Pro__sink --layout 5.1
cargo run -- lpcm-check --sink alsa_output.YOUR_HDMI_CARD.HiFi__Pro__sink --layout 7.1
```

The command validates the sink's channel map and, for a Pro Audio sink with AUX positions, confirms the associated HDMI port is connected. It then plays one quiet 24-bit/48 kHz tone at a time. The printed order includes each channel name and frequency so you can record where each tone is heard. It uses temporary PipeWire nodes and does not change the system default sink or application routes. This is a manual listening check of the HDMI LPCM leg; it does not verify the Wi-Fi RTP path or acoustic synchronization. Inspect ALSA ELD as well, especially when using a standard HDMI sink, and preview the route with `--dry-run` first.

## Current verification limits

The Rust acceptance tests check 5.1/7.1 channel maps, RTP module plans, isolated LPCM channel-test data, PipeWire sink parsing, Pro Audio HDMI-port availability, rejection of incompatible outputs, and analysis of acoustic sync observations. The host-only RTP acceptance script sends a distinct tone on every channel through a simulated AUX sink and checks channel identity after the receiver mapping. The `lpcm-check` command has also been exercised against a temporary virtual 8-channel sink; this verifies the CLI and PipeWire routing, not Q900T playback. On this host, NVIDIA HDMI-A-5 carries video to a TCL “Beyond TV”, but the GPU HDMI ELD entries are invalid. The display EDID advertises only a two-channel LPCM format. Forcing the NVIDIA 7.1 HDMI profile creates an eight-channel sink without a usable speaker map, so the receiver correctly refuses it; this does not test or establish Q900T playback. The Q900T itself is visible only as a two-channel Bluetooth sink; cross-host Wi-Fi, HDMI LPCM to the soundbar, and acoustic synchronization remain unverified in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12) and [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

Acceptance scenarios are recorded in [`acceptance/multichannel.feature`](acceptance/multichannel.feature). Research reports are stored under `research/` on short-lived `research/<topic>` branches and linked from the relevant decision issues.
