# Samsung Q900T synchronized Linux audio

An experimental Rust bridge that routes Linux system audio through PipeWire to multiple selected audio outputs in parallel, with a fixed delay for each output. Its RTP sender and receiver also transport uncompressed PCM over Wi-Fi in stereo (2 channels), 5.1 (6 channels), or 7.1 (8 channels) at 24-bit/48 kHz. The RTP path avoids Bluetooth codecs such as SBC.

The immediate implementation target is simple system-wide stereo fan-out to any selected PipeWire sinks, with per-output delay values. The user confirmed that host HDMI cannot connect directly to the Q900T. The host motherboard is an MSI MAG Z690 TOMAHAWK WIFI DDR4 with an optical S/PDIF output, but no TOSLINK cable currently connects it to the Q900T, so optical playback is unverified. The connected Q900T Bluetooth route negotiates SBC; use it as a routing baseline only. See [the optical capability report](research/q900t-optical-spdif.md).

The user wants microphone-based automatic delay measurement. PipeWire exposes USB Audio Mic/Line and Logitech BRIO capture sources; the user reports a microphone is positioned about one metre above the soundbar. Automatic recording, signal correlation, and delay application are not implemented yet. The first fan-out version only supports manually supplied fixed delays. See the [calibration implementation issue](https://github.com/guility/samsung-q900-wifi-linux/issues/15) and [synchronization decision](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

The wireless leg uses raw linear PCM rather than a lossy audio codec, so it preserves the decoded audio samples and discrete channels. Its audio payload is 2.304 Mbit/s for stereo, 6.912 Mbit/s for 5.1, and 9.216 Mbit/s for 7.1, before RTP/UDP/IP overhead. The same-host loopback checks channel preservation; sustained delivery across two Wi-Fi hosts still needs acceptance testing.

When multichannel is tested, 5.1 or 7.1 means six or eight discrete PCM input channels. Samsung's 7.1.2 label describes the Q900T speaker configuration; it does not by itself confirm that HDMI accepts discrete 7.1.2 LPCM. This MVP does not claim independent height-channel input, and the immediate hardware milestone is stereo.

The destination and open product decisions are tracked in the [Wayfinder map](https://github.com/guility/samsung-q900-wifi-linux/issues/1). Implementation work is tracked in [issue #10](https://github.com/guility/samsung-q900-wifi-linux/issues/10); direct Q900T HDMI LPCM acceptance is pending in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12).

## Requirements

- Linux with PipeWire, its PulseAudio compatibility server (`pactl`), and the `pw-loopback` client.
- IPv4 multicast enabled on the Wi-Fi LAN. The default RTP/SAP address is `224.0.0.56`, with RTP port `5004`.
- On hosts with multiple network interfaces, pass `--source-ip` with the IPv4 address assigned to Wi-Fi (for example, find it with `ip -4 addr show dev wlan0`). If omitted, the system routing table chooses the outbound interface. PipeWire's RTP sender supports this source address setting ([module-rtp-send](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_send.html)).
- The receiver must expose an active PipeWire sink with the requested channel count. Stereo RTP requires ordered `FL`/`FR` positions; it can target a stereo S/PDIF sink if a receiver has one. Multichannel HDMI profiles must report the expected speaker positions. For a Pro Audio sink with `AUX0…AUX7` positions, the matching HDMI card port must also report connected; a Pro Audio profile alone does not prove an HDMI device is present.
- To connect the multichannel receiver to a Q900T, HDMI must negotiate 5.1/7.1 LPCM; that has not yet been verified on the available hardware. Samsung lists `SBC` as the Q900T Bluetooth codec and describes Wi-Fi as “Wi-Fi TV Connection” for compatible Samsung TVs. It also lists Spotify Connect, which Spotify documents as playback of Spotify content selected from its app, not as a generic Linux system-audio input; the sources do not specify multichannel mapping or latency for that feature. The host exposes Q900T over two-channel A2DP with `SBC` and `SBC-XQ` profiles. The multichannel RTP sender rejects that Bluetooth sink; the separate `mirror` command can duplicate stereo to it, but its active A2DP profile may use SBC and therefore does not meet the above-SBC quality requirement. Samsung's specification sheet lists HDMI inputs and Dolby/DTS decoders but does not confirm discrete 5.1/7.1 LPCM, so the multichannel receiver path remains conditional on hardware testing ([Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/), [Spotify Connect docs](https://support.spotify.com/us/article/spotify-connect/), [official specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf)). Current discovery results and the limits of that inference are in [`research/q900t-wifi-endpoints.md`](research/q900t-wifi-endpoints.md).
- The stereo `mirror` command accepts two or more selected PipeWire output sinks, including Bluetooth, analog/speaker, HDMI, and S/PDIF sinks, provided they expose the stereo channels. Bluetooth A2DP may use SBC. Optical S/PDIF carries stereo PCM; the host connector exists, but its TOSLINK cable is not connected to the Q900T.

## Parallel stereo output

List available PipeWire outputs, then pass each chosen sink as a repeated `--output` argument. Append `=DELAY_MS` to apply a fixed delay to that output; omitted delays default to zero:

```sh
pactl list short sinks

cargo run -- mirror \
  --output YOUR_LOCAL_SINK=60 \
  --output bluez_output.44_5C_E9_1D_E9_51.1=0 \
  --dry-run
```

Remove `--dry-run` to route all system audio through the temporary stereo bus to every selected output. The command restores the prior default sink and stream routes on Ctrl-C. Each output must expose FL/FR; the CLI rejects duplicate sinks and requires at least two outputs. `--latency-ms` sets the common PipeWire buffer, while each `=DELAY_MS` adds an independent fixed delay from 0 to 2000 ms. This version does not measure delays automatically and does not compensate clock drift.

The optical sink on this host is `alsa_output.usb-Generic_USB_Audio-00.HiFi__SPDIF__sink` (`s16le`, 2 channels, 48 kHz). The motherboard has a physical optical connector, but the user reports that no TOSLINK cable connects it to the Q900T. `lpcm-check` can verify that PipeWire sends separate FL/FR tones to the sink, but that is not an audible Q900T acceptance check until a cable is connected.

```sh
cargo run -- lpcm-check \
  --sink alsa_output.usb-Generic_USB_Audio-00.HiFi__SPDIF__sink \
  --layout stereo
```

The command confirms PipeWire accepted the channel test. It does not prove that an endpoint produced sound. The host can still route stereo directly to Q900T over Bluetooth, but its active A2DP codec is SBC.

## Automatic microphone calibration

The preferred next step is to play a known probe on each selected output, capture all arrivals with one microphone clock, estimate each path's acoustic delay, and apply fixed offsets to the earlier outputs. The user reports that a microphone is mounted about one metre above the Q900T. The host currently exposes two possible capture sources, but the exact microphone source has not been identified and no calibration recording has been made. Calibration at one microphone position aligns outputs there; room changes, movement, and long-term clock drift need separate handling. The implementation plan is tracked in [issue #15](https://github.com/guility/samsung-q900-wifi-linux/issues/15), with measurement criteria in [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

## Stereo RTP over Wi-Fi

Use `--layout stereo` on both RTP endpoints to send raw 24-bit stereo PCM without the Q900's A2DP codec:

```sh
# Source host
cargo run -- sender --layout stereo \
  --destination-ip 224.0.0.56 \
  --local-sink YOUR_LOCAL_STEREO_SINK

# Linux receiver with a stereo output (for example, an optical S/PDIF sink)
cargo run -- receiver --layout stereo --sink YOUR_RECEIVER_STEREO_SINK
```

If a Linux receiver's optical output is connected to the Q900T, the host-to-receiver leg stays wireless and the final receiver-to-soundbar leg is wired. The current host exposes an S/PDIF sink as 16-bit/48 kHz stereo and its motherboard has an optical S/PDIF connector, but the user says no TOSLINK cable is connected to the Q900T. Direct optical playback, cross-host Wi-Fi delivery, automatic microphone calibration, and acoustic synchronization remain unverified. The decision about a separate Wi-Fi receiver remains open in [issue #14](https://github.com/guility/samsung-q900-wifi-linux/issues/14).

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
bash acceptance/loopback.sh stereo
```

The loopback script creates temporary virtual sinks, sends a distinct tone on every channel, records the receiver sink, checks that each channel contains audio, and cleans up the sinks and routes.

## Analyze acoustic synchronization measurements

The `sync-report` command analyzes repeated acoustic arrival measurements from a CSV file:

```sh
cargo run -- sync-report --input acceptance/sample-sync-observations.csv
```

The required columns are `elapsed_s,local_ms,q900_ms`. Both arrival columns must use the same recorder timebase; use a single microphone/recorder clock at the listening position. The example file is synthetic and demonstrates an 8 ms fixed offset only. The report fits fixed offset and linear drift for one local/Q900T pair, summarizes absolute skew and residual jitter, and recommends which output to delay. This input schema is still pair-specific; automatic microphone capture and multi-output analysis are planned.

Apply a fixed-delay recommendation in `mirror` by appending `=N` to that output's `--output SINK` argument. These use PipeWire's explicit `pw-loopback --delay` control and accept 0–2000 ms. They compensate a fixed acoustic offset; they do not set it automatically or correct clock drift. The RTP receiver uses PipeWire's default constant-latency mode, which adjusts buffer consumption to track sender/receiver clock-rate differences. That mechanism does not guarantee acoustic alignment, so skew and drift still need measurement. The `--latency-ms` setting controls the RTP buffer target, not added delay or a synchronization guarantee ([PipeWire RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html), [RTP internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html)).

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

Acceptance scenarios are recorded in [`acceptance/multichannel.feature`](acceptance/multichannel.feature). Research reports live under `research/` and link to the relevant decision issues; focused research-only work may use short-lived `research/<topic>` branches.
