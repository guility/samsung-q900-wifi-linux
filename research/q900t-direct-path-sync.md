# Q900T wireless input and synchronized playback review

Research date: 2026-11-19

Scope: determine whether the HW-Q900T itself is a usable generic Linux Wi-Fi/Bluetooth endpoint for higher-quality 5.1/7.1 audio, and identify a realistic synchronization path for a PipeWire/Rust bridge.

## Decision

No documented Q900T-native Bluetooth or Wi-Fi input is established for arbitrary Linux system audio in above-SBC 5.1/7.1 form.

- **Bluetooth is ruled out for this requirement.** Samsung lists SBC as the Q900T Bluetooth codec; the host currently connects the bar as a two-channel A2DP sink. SBC's defined channel modes are mono, dual-channel, stereo, and joint stereo. The optional PipeWire `sbc_xq` profile is still SBC and does not add channels. ([Samsung Q900T specifications](https://www.samsung.com/at/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-en/), [Bluetooth SIG A2DP v1.4.1 §4.3.2.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html))
- **The documented Wi-Fi audio route is TV-specific.** The Q900T manual says its Wi-Fi TV connection is available only on Samsung TVs, with TV and soundbar on the same network. Q-Symphony itself is documented for selected Samsung TVs and requires HDMI or optical between TV and soundbar. Samsung lists Wi-Fi services such as SmartThings and Spotify Connect, but does not document a generic Linux audio-renderer interface for arbitrary applications. The last statement is an inference from Samsung's listed interfaces, not proof that no private protocol exists. ([Q900T manual, §§5–6, pp. 23, 26, 32](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf), [Samsung Q900T specifications](https://www.samsung.com/at/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-en/))
- **A Wi-Fi-to-Linux-receiver-to-HDMI path remains the strongest candidate**, because it can transport uncompressed multichannel PCM over the LAN and feed the soundbar over HDMI. This depends on actual 5.1/7.1 LPCM acceptance by the Q900T. Samsung documents HDMI Dolby/DTS playback formats and an output-mode table containing 5.1/7.1 input labels, but does not explicitly state 5.1/7.1 LPCM input support or PCM limits. A 7.1.2 speaker layout is not proof of 7.1 LPCM input. ([Q900T manual, §§7 and 16, pp. 27, 44](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf), [Samsung Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf))

The receiver prototype is therefore a conditional bridge, not proof that the soundbar accepts its PCM. Test that HDMI format on the actual Q900T before claiming a working 5.1/7.1 product path.

## What the Q900T documentation establishes

| Input | Documented behavior | Fit for arbitrary Linux audio |
| --- | --- | --- |
| Bluetooth A2DP | Samsung identifies SBC for the Q900T. The manual describes its Bluetooth TV connection as stereo. | Fails the user's above-SBC and 5.1/7.1 requirement. |
| Wi-Fi TV Connection | Requires a Samsung TV; TV and soundbar must be on the same Wi-Fi network. | No generic Linux endpoint or arbitrary system-audio API is documented. |
| Spotify Connect / SmartThings | Listed as Q900T services; Wi-Fi setup and control use Samsung's app/service flow. | Music/service control does not cover every application's audio, video, or games. |
| HDMI | Samsung lists Dolby Atmos/TrueHD, Dolby Digital Plus, Dolby Digital, DTS-HD Master Audio, DTS:X, and DTS playback. The manual restricts Atmos/DTS:X to HDMI mode. | Best documented high-quality input, but multichannel LPCM negotiation is not settled by the manual. |

The manual's sound-mode table labels 2.0, 5.1, 7.1, and 7.1.4 input channel counts. It explains the soundbar's processing/output modes; it does not say those rows are HDMI LPCM input configurations. The manual's “Supported play formats” list is Dolby/DTS formats, with no explicit LPCM line. Therefore, the RTP bridge's raw PCM over Wi-Fi followed by HDMI LPCM needs a hardware test. If PCM fails, a real-time encoded bitstream alternative also needs separate validation for live audio, channel count, quality, and latency.

## Synchronization path and limits

PipeWire provides two useful pieces:

1. **Local graph fan-out and latency compensation.** `libpipewire-module-combine-stream` creates one virtual sink that forwards to selected sinks. `combine.latency-compensate` inserts delay buffers to match the stream latencies PipeWire sees. That is useful for system-wide routing and for endpoints reporting their delay. It cannot correct an unreported Q900T DSP delay or prove acoustic alignment at a listening seat. ([PipeWire Combine Stream](https://pipewire.pages.freedesktop.org/pipewire/page_module_combine_stream.html))
2. **Timestamped RTP with a shared clock.** Native PipeWire `libpipewire-module-rtp-sink/source` supports raw PCM, a target transport-latency buffer, and direct-timestamp mode. Constant-latency mode keeps the receive buffer near a target using rate correction; the documentation explicitly says it does not guarantee playback in sync with other devices. Direct-timestamp mode can synchronize receivers only when their graph drivers use a time base synchronized to the sender; in that mode, `sess.latency.msec` is an assumed maximum transport delay. PipeWire's AES67 configuration supports a PTP-driven graph and a separate PTP sender. The separate sender can leave local graph timing alone, but adds about the configured target latency minus one quantum and does not configure the receiver for PTP. ([RTP source and buffer modes](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html), [RTP sink and PTP sender](https://docs.pipewire.org/devel/page_module_rtp_sink.html), [RTP internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html))

RTP timestamps alone do not give unrelated streams a shared clock. RFC 3550 uses RTCP Sender Reports to map RTP timestamps to a common wall-clock reference for synchronization; sources must actually send and receivers must use that mapping. ([RFC 3550 §§5.1, 6.4](https://datatracker.ietf.org/doc/html/rfc3550))

### Current prototype and host evidence

At commit [cf971eb](https://github.com/guility/samsung-q900-wifi-linux/commit/cf971eb), the Rust CLI loads PulseAudio-compatible `module-rtp-send` / `module-rtp-recv` modules and uses a nominal 100 ms latency setting. That verifies the local PipeWire RTP graph and channel mapping, but it does not implement a PTP-synchronized presentation contract or measure the Q900T acoustic path. ([prototype source](https://github.com/guility/samsung-q900-wifi-linux/blob/cf971eb/src/lib.rs), [PipeWire Pulse RTP sender options](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_send.html), [PipeWire Pulse RTP receiver options](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_recv.html))

On the host checked for this report, PipeWire is 1.6.11 and `pipewire-aes67` is installed. `ptp4l` and `phc2sys` are not on the executable path, no PTP-synchronized receiver host was available, and no PTP stream was tested. The Q900T is currently connected as a 2-channel Bluetooth sink with A2DP/SBC active. The NVIDIA HDMI ELD entries report `monitor_present=0` and `eld_valid=0`, so there is no current HDMI handshake with the soundbar.

For reliable synchronization, use the clock/timestamp mechanisms to manage packet timing and clock drift, then calibrate the **acoustic** offset. A shared PTP clock does not remove the receiver's HDMI buffering, Q900T processing, or room propagation delay. Measure both outputs with one microphone/ADC clock at a named listening seat; record fixed skew, drift, jitter, dropouts, and reconnect behavior. Check audio-to-picture delay separately for video, and event/input-to-sound latency separately for games. The project's existing [sync calibration report](https://github.com/guility/samsung-q900-wifi-linux/blob/research/sync-calibration/research/sync-calibration.md) proposes 5 ms as a conservative candidate audio-to-audio skew target, not a standards limit.

## Recommended next step

1. Connect a host HDMI output directly to Q900T HDMI 1 or 2 and check ELD, ALSA, and PipeWire negotiation.
2. Send discrete 5.1 and 7.1 LPCM channel-identification signals. Record sample format, rate, channel order, and whether the bar plays each channel correctly. This decides whether the current PCM receiver architecture can reach the Q900T at all.
3. If LPCM passes, keep the host's local output in the graph and test the receiver path on a second Linux host. For timing work, compare the current Pulse-compatibility RTP plan with native RTP modules and PipeWire's PTP/direct-timestamp mode. Measure the resulting acoustic skew before exposing a “synchronized” feature.
4. If LPCM fails, treat HDMI bitstream encoding as a separate design decision. The Q900T's listed Dolby/DTS decoders do not prove that a Linux real-time encoder will preserve the requested 5.1/7.1 content or keep it responsive for games.

## Primary sources

- [Samsung HW-Q900T user manual, version 2402130 (official PDF)](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf)
- [Samsung HW-Q900T product specifications](https://www.samsung.com/at/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-en/)
- [Samsung HW-Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf)
- [Bluetooth SIG, Advanced Audio Distribution Profile v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html)
- [PipeWire RTP sink](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_sink.html); [RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html); [RTP internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html)
- [PipeWire Combine Stream](https://pipewire.pages.freedesktop.org/pipewire/page_module_combine_stream.html); [PipeWire Pulse RTP sender](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_send.html); [PipeWire Pulse RTP receiver](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_rtp_recv.html)
- [IETF RFC 3550, RTP](https://datatracker.ietf.org/doc/html/rfc3550)
