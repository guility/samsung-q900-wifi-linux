# Wi-Fi-to-HDMI bridge for Q900T multichannel audio

Research date: 2026-11-19

## Recommendation

A Wi-Fi receiver beside the soundbar is a realistic architecture: send multichannel raw PCM from the Linux host over RTP, receive it in PipeWire on a small Linux computer, and send it out over that computer's HDMI port. Raw PCM avoids Bluetooth SBC compression and can carry 5.1 or 7.1 channel samples. PipeWire exposes RTP channel count, channel positions, sample rate, and sample format, and its receiver accepts raw PCM RTP.

This is a **conditional path, not a verified Q900T solution**. Samsung's current Q900T manual does not state that either HDMI input accepts 5.1/7.1 LPCM. It documents Dolby/DTS playback and an output-mode table with 5.1/7.1 input labels, but the table does not identify the connection or codec. Before buying or implementing a receiver, test 6-channel and 8-channel LPCM on the actual Q900T HDMI input. If that fails, this research did not verify a Linux software path that both encodes arbitrary live host audio into a supported Dolby/DTS bitstream and preserves the requested channel count and quality.

**Candidate receiver:** Raspberry Pi 5 running Raspberry Pi OS Lite with PipeWire. It has dual-band Wi-Fi, gigabit Ethernet, and two 4Kp60 HDMI outputs; Pi documentation says HDMI audio is enabled by default. Raspberry Pi OS documents a `Pro Audio` profile that exposes the maximum channel count across audio devices, which is the profile to investigate for HDMI multichannel output. That is useful OS support, but it does not promise that a given HDMI sink negotiates 6/8-channel LPCM; the Q900T EDID and actual channel output still need testing. A 1GB board is currently listed by Raspberry Pi at $45 US; that price excludes power supply, microSD, case/cooling, and a micro-HDMI cable/adapter, and local reseller pricing/availability varies. Do not purchase until the Q900T LPCM test has a result.

## What Samsung documents

The model-specific Samsung product sheet lists HW-Q900T/ZA as 7.1.2-channel, with two HDMI inputs and one HDMI output, 4K/60Hz video pass-through, and Dolby Atmos, Dolby TrueHD, Dolby Digital Plus, DTS:X, and DTS 5.1 support. Samsung's 2024 manual (version 2402130) says Dolby Atmos and DTS:X are supported in HDMI mode only and tells a player to use unprocessed bitstream for Atmos. The manual's supported-play-formats list includes Dolby Atmos (TrueHD/Digital Plus), Dolby Digital, DTS-HD Master Audio, DTS:X, and DTS.

The manual's “Output specifications for the different sound effect modes” table labels its input rows 2.0, 5.1, 7.1, and 7.1.4 channels. This is an effect-mode/output table; it does not map those rows to an HDMI input or state that they are LPCM. I found no Q900T HDMI input audio-format table or explicit 5.1/7.1 LPCM statement in the current manual. Therefore:

- **Documented:** the Q900T has HDMI inputs and advertises the listed Dolby/DTS playback formats; Dolby Atmos and DTS:X use HDMI mode.
- **Not established by the manual:** 5.1- or 7.1-channel LPCM over HDMI, the maximum LPCM sample rate/bit depth, or whether a Linux receiver's PCM HDMI output will negotiate successfully.
- **Do not infer:** its 7.1.2 speaker layout or the effect-mode input labels are proof of a 7.1 LPCM HDMI input.

This boundary matters: the proposed RTP path carries PCM, not an HDMI Dolby/DTS bitstream. If Q900T rejects multichannel PCM, a working bridge would need a separately validated real-time bitstream-encoding or passthrough path; that alternative is not established here.

Sources: [Samsung HW-Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf); [Samsung HW-Q900T support page and current manual listing](https://www.samsung.com/uk/support/model/HW-Q900T/ZF/); [2024 HW-Q900T manual, version 2402130 (PDF)](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM/202402/20240215110810001/AH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf).

## Receiver architecture

1. Keep the host's existing local audio output active.
2. Route the same PipeWire program audio to an RTP sink. Configure raw PCM, 48kHz, and a standard 6-channel or 8-channel layout (for example FL/FR/FC/LFE/SL/SR, plus RL/RR for 7.1). Use unicast to the receiver's address for a single room, or verify multicast behavior on the Wi-Fi access point.
3. On the Pi or another Linux receiver, create a PipeWire RTP source and route it to the receiver's HDMI playback device.
4. Connect that HDMI output to Q900T HDMI 1 or 2 and test channel mapping, LPCM acceptance, and actual acoustic output.

PipeWire's official RTP sink documentation lists `audio.channels`, `audio.position`, `audio.rate`, and `audio.format` as stream properties, and gives a configurable destination IP/port and 2–20ms packet-time bounds. Its RTP source documentation says received packets may contain raw PCM, Opus, or MIDI; the source implementation has a configurable latency buffer and constant-latency mode with rate adjustment. These documents establish a Linux network-audio seam, but do not promise that a particular Pi HDMI driver accepts eight channels. Confirm the sender and receiver expose the same channel count and order before the Q900T test.

At 48kHz/24-bit, raw payload is about 6.9Mbit/s for 5.1 and 9.2Mbit/s for 7.1, before RTP/IP/Wi-Fi overhead. This is a reasonable data rate for a nearby, stable 5GHz Wi-Fi link; packet loss and jitter still need testing. Ethernet from host to access point, with Wi-Fi only on the receiver leg, is also an option if the user means wireless delivery at the soundbar rather than a Wi-Fi radio on both endpoints.

Sources: [PipeWire RTP sink](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_sink.html); [PipeWire RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html); [PipeWire RTP module internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html); [Raspberry Pi 5 specifications](https://www.raspberrypi.com/products/raspberry-pi-5/); [Raspberry Pi HDMI audio configuration](https://www.raspberrypi.com/documentation/computers/config_txt.html#hdmi-audio); [Raspberry Pi OS audio device profiles](https://github.com/raspberrypi/documentation/blob/master/documentation/asciidoc/computers/configuration/audio-display.adoc); [Raspberry Pi 5 pricing announcement](https://www.raspberrypi.com/news/1gb-raspberry-pi-5-now-available-at-45-and-memory-driven-price-rises/).

## Latency and synchronization

RTP packetization, receiver buffering, HDMI buffering, and Q900T processing all add delay. PipeWire's RTP packet-time settings are only one part of that path; the chosen receiver latency buffer is separate. In constant-latency mode PipeWire adjusts playback rate to maintain a buffer target, while its direct-timestamp mode assumes synchronized graph clocks. Neither mode provides an end-to-end Q900T latency guarantee.

The host can fan audio to its local sink and the RTP sink, but matching audible arrival times needs measurement. Measure fixed skew and drift at the listening position, then compensate the host-local route or receiver delay. Repeat during a long run and under Wi-Fi load, checking dropouts, channel order, and reconnect behavior. The Q900T manual offers a 0–300ms SYNC setting for some functions; it does not establish that this control corrects an external receiver's audio-to-audio skew. A game-friendly synchronization target cannot be promised from specifications alone.

## Current host baseline

Read-only checks on the current Linux host found:

- Linux x86_64, NVIDIA GB202/RTX 5090 GPU with an NVIDIA HD Audio Controller, plus Intel UHD Graphics 770/onboard audio.
- `xrandr` shows NVIDIA `HDMI-A-5` connected as the primary display at 3840×2160 and about 143.94Hz. The prior live EDID/PipeWire inspection identified that display as a TCL device named “Beyond TV” (product 0x9653), not a Samsung TV.
- PipeWire currently exposes the Q900T as a Bluetooth output and shows USB outputs; it does not expose an HDMI sink connected to the Q900T. The current HDMI cable goes to the TCL display. The NVIDIA card lists 5.1 and 7.1 HDMI output profiles, but every HDMI port is currently unavailable and the card profile is `off`; these profiles show host-side support, not Q900T acceptance. The `aplay` utility is not installed, so ALSA channel capabilities were not queried.
- Other enumerated GPU connectors are disconnected. The host has HDMI-capable audio hardware, but no direct Q900T HDMI channel test has been performed. A second output or a separate receiver is needed to keep the current local display path while independently feeding the soundbar. Using the Q900T's video pass-through inline could cap the current display path at the soundbar's documented 4K/60Hz pass-through rate.

These observations do not rule out a direct HDMI test. They show that the present HDMI/EDID negotiation is with the TCL display, not the Q900T, and the current PipeWire session has no Q900T HDMI audio endpoint. The wireless receiver architecture avoids replacing that local display connection.

## Remaining tests before choosing the path

1. Connect the host's free HDMI output to Q900T HDMI 1/2, inspect the receiver's HDMI ELD/ALSA/PipeWire profile, and play discrete 5.1 and 7.1 LPCM channel-identification samples. Record rate, bit depth, channel order, and whether Q900T reports HDMI audio. This can test Q900T LPCM acceptance before adding a separate Wi-Fi receiver.
2. On a Pi 5 or another prospective receiver, select the PipeWire Pro Audio profile and verify the connected Q900T sink negotiates 6 and 8 channel LPCM under Linux. Raspberry Pi OS documents Pro Audio as exposing the maximum channel count, but don't treat that generic profile or a stereo output as proof of multichannel HDMI support.
3. Only after both tests pass, stream multichannel PCM over RTP and measure host-local versus Q900T skew, drift, jitter, packet loss, and audio/video delay. Repeat on the intended Wi-Fi network.
4. If Q900T does not accept multichannel LPCM, stop and decide whether the scope permits a documented compressed 5.1 Dolby/DTS route. A software encoder's existence or a listed decoder format alone does not verify live 5.1/7.1 quality, channel preservation, or sync.

## Context from the prior Q900T multichannel report

The closed issue [Identify a higher-quality 5.1/7.1 wireless path to Q900T](https://github.com/guility/samsung-q900-wifi-linux/issues/8) concluded that built-in Bluetooth A2DP/SBC is stereo and that Samsung's Wi-Fi TV Connection is documented for compatible Samsung TVs, not a generic Linux renderer. Its [report](https://github.com/guility/samsung-q900-wifi-linux/blob/research/q900t-multichannel/research/q900t-multichannel.md) also flags multichannel HDMI LPCM as unverified. I used this report as project context; the hardware and protocol claims above are cited to Samsung, Raspberry Pi, and PipeWire primary documentation.
