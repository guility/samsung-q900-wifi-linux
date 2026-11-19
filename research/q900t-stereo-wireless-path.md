# Q900T stereo wireless path without HDMI

**Checked:** 2026-11-19
**Decision:** No documented or host-verified direct path currently meets the user's requirement for arbitrary Linux system audio to reach this Q900T over Wi-Fi or a non-SBC Bluetooth codec. The Q900T's optical input and the host's enumerated USB S/PDIF output suggest a possible **wired stereo PCM** fallback, but its physical connection and acoustic behavior have not been verified.

## Direct Bluetooth and Wi-Fi

Samsung's [HW-Q900T product page](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/) lists Bluetooth codec **SBC**, Wi-Fi, Spotify Connect, one optical input, no USB music playback, and no Ethernet port. Its listed MP3/AAC/FLAC/WAV/ALAC/AIFF decoding formats describe formats the product can decode; they do not identify a generic network PCM input. Samsung's [specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf) likewise lists Wi-Fi and optical/HDMI connections without specifying an open Linux audio endpoint.

The [Q900T user manual](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf) says its Wi-Fi TV connection is available only with a Samsung TV. Its SmartThings instructions describe adding the soundbar to a wireless network, not sending arbitrary PCM from a Linux host. Samsung lists Spotify Connect, but [Spotify's developer guide](https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/connect-basics) describes transferring Spotify playback to the speaker's application while the phone acts as a remote. That is not a documented sink for captured desktop audio or all system sounds.

A2DP allows optional codecs, but the [Bluetooth SIG profile](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html) requires both endpoints to support an optional codec; SBC is mandatory. Samsung lists only SBC for this Q900T. PipeWire/WirePlumber supports codecs including AAC, LDAC, and aptX on the Linux side, but that cannot add a codec the soundbar does not advertise. The host's `sbc_xq` option is explicitly a high-quality **SBC** mode, not a different codec ([WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html)).

Current host evidence agrees: PipeWire exposes the Q900T as `s16le 2ch 48000Hz` over A2DP, with `a2dp-sink` active and an `a2dp-sink-sbc_xq` profile available. Read-only mDNS discovery found no `_spotify-connect._tcp` or `_samsungmsf._tcp` service. The AirPlay/RAOP services found belonged to a MacBook and Apple TV, not the Q900T. Discovery is a snapshot of this LAN and does not rule out undocumented or inactive firmware behavior.

## Optical stereo fallback

Samsung's manual documents connecting an external device's optical output to the soundbar's `DIGITAL AUDIO IN (OPTICAL)` and selecting `D.IN`. It also states that Dolby Atmos and DTS:X are supported in HDMI mode only. The manual does not establish 5.1/7.1 LPCM support over optical.

This host currently enumerates `USB Audio S/PDIF Output` in PipeWire as `s16le 2ch 48000Hz`; its state is `SUSPENDED`, and the card reports the S/PDIF port's availability as **unknown**. If that USB device has a physical optical output and a TOSLINK cable is available, it could feed the Q900T directly with stereo PCM, avoiding the SBC encode/decode step. This is a wired, stereo-only candidate outside the final Wi-Fi/Bluetooth objective. Physical connector availability, playback, and acoustic alignment are unverified.

A separate Wi-Fi receiver running the project's RTP/PCM receive path with an optical-capable output is also technically plausible: the host could keep its local output active while sending PCM to the receiver, which feeds the soundbar's optical input. It requires an additional receiver/output device and an optical cable, and the observed host-style S/PDIF endpoint is stereo only. PipeWire's [RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html) exposes a configurable receive buffer and clock-rate tracking, but its [RTP internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html) do not promise acoustic synchronization with the Q900T. Receiver buffering, USB/optical output, and soundbar DSP latency need measurement and fixed-delay calibration; no sync result can be inferred from the software graph.

## Conclusion and next evidence

- Do not treat the current A2DP/SBC mirror as meeting the above-SBC requirement. SBC-XQ remains SBC and the observed sink is stereo.
- No generic Q900T Wi-Fi system-audio endpoint or non-SBC Bluetooth codec is documented or discovered for this host. Spotify Connect is a Spotify playback target, not a general system PCM route.
- For a temporary stereo experiment without HDMI, first verify whether the host USB Audio S/PDIF connector is physically usable, then connect it to the Q900T optical input. This would be wired stereo PCM, not the final wireless result.
- A Wi-Fi-to-optical receiver is a plausible extension if direct host optical is unavailable, but it adds hardware and still does not establish multichannel LPCM or acceptable game/video/music synchronization.

## Sources

- Samsung, [HW-Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/).
- Samsung, [HW-Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf).
- Samsung, [HW-Q900T user manual](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf), sections on Wi-Fi TV, optical input, and supported HDMI formats.
- Bluetooth SIG, [Advanced Audio Distribution Profile 1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html).
- Spotify for Developers, [Spotify Connect Basics](https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/connect-basics).
- WirePlumber, [Bluetooth configuration and codec options](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html).
- PipeWire, [RTP source](https://pipewire.pages.freedesktop.org/pipewire/page_module_rtp_source.html) and [RTP module internals](https://pipewire.pages.freedesktop.org/pipewire/devel/page_rtp_module_internals.html).
