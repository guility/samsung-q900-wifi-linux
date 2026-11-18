# Q900T wireless playback transport for Linux

Research date: 2026-11-19  
Scope: choose the first wireless path for a Rust/Linux MVP that routes host audio to both a local output and one Samsung HW-Q900T.

## Recommendation

Use **Bluetooth A2DP through BlueZ and PipeWire** as the MVP's first remote output. The project has already observed the Q900T paired and connected as an Audio Sink and exposed as a PipeWire Bluetooth output; this establishes a working host-side route, though the connection and negotiated codec must be rechecked before implementation. ([issue 7](https://github.com/guility/samsung-q900-wifi-linux/issues/7))

The Q900T's Wi-Fi features are real, but Samsung documents them as SmartThings setup/control, Spotify Connect, and a Wi-Fi connection to a compatible Samsung TV. The docs do not establish a generic Linux audio sink or a documented UPnP/DLNA, AirPlay, or raw PCM receiver. That makes Wi-Fi an unproven, service-specific path for arbitrary host audio. This is an inference from the interfaces Samsung documents; it is not a claim that the speaker cannot use any other network protocol. ([Samsung HW-Q900T specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy), [Samsung Q900T full manual](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_AU&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=7783523&CDCttType=UM&VPath=UM%2F202008%2F20200810142710861%2FAH81-13331R-00_FM_HW-Q900T_XL_XS_XM_XY_XP_ENG_200727.pdf))

## Manufacturer-confirmed capabilities

Samsung's HW-Q900T specification lists Bluetooth, **SBC** as its Bluetooth codec, Wi-Fi, SmartThings, and Spotify Connect. It lists no Ethernet port and no USB music playback. Samsung also describes Wi-Fi TV Connection as connecting a compatible Samsung TV to the soundbar. The full manual's Wi-Fi mobile-device procedure requires the SmartThings app. These are distinct documented network services; the specification and manual do not describe Wi-Fi as a general computer audio output. ([Samsung HW-Q900T specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy), [Samsung Q900T full manual, §§6 and 8](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_AU&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=7783523&CDCttType=UM&VPath=UM%2F202008%2F20200810142710861%2FAH81-13331R-00_FM_HW-Q900T_XL_XS_XM_XY_XP_ENG_200727.pdf))

The Wi-Fi TV feature is explicitly scoped to a compatible Samsung TV. SmartThings is an app-mediated network setup/control path, and Spotify Connect is a named service feature. Samsung does not document a generic Linux sender API for those features. A Linux program might be able to control a supported service, but that would not by itself provide a sink for arbitrary desktop/game audio.

## Bluetooth A2DP and Linux routing

A2DP models a digital audio **Source** delivering a stream to an audio **Sink**. The Bluetooth SIG requires SBC codec interoperability for A2DP; other codecs are optional. Samsung's Q900T spec identifies SBC as its Bluetooth codec, so SBC is the known common codec for this link. Do not assume AAC, LDAC, or a lossless Bluetooth path from the soundbar's Wi-Fi or 7.1.2 channel count. ([Bluetooth SIG, A2DP v1.4.1, §§2.2 and 4.2–4.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html), [Samsung HW-Q900T specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy))

BlueZ documents A2DP media-endpoint negotiation and codec capabilities, and its `btmon` documentation describes A2DP/AVDTP codec negotiation and media transport. WirePlumber's BlueZ monitor watches connected Bluetooth audio devices and creates PipeWire device/node objects; its documented roles include A2DP and its codec list includes SBC. In practical terms, the host can feed the Q900T like another PipeWire output and fan the same graph stream to the local output. The already-observed PipeWire output is stronger feasibility evidence for this MVP than the Q900T's generic Wi-Fi capability. ([BlueZ MediaEndpoint API](https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.MediaEndpoint.rst), [BlueZ A2DP `btmon` docs](https://raw.githubusercontent.com/bluez/bluez/master/doc/btmon-a2dp.rst), [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html))

Samsung only confirms SBC for this model. Treat the Bluetooth input as an A2DP SBC stream (at most two audio channels in SBC); the advertised 7.1.2 describes the soundbar's speaker system, not a promise of 7.1.2 channels over Bluetooth. Let the soundbar handle its own processing, then test its selected sound mode separately. ([Bluetooth SIG, A2DP v1.4.1, §4.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html), [BlueZ A2DP codec/channel capability example](https://raw.githubusercontent.com/bluez/bluez/master/doc/btmon-a2dp.rst))

## Latency, control, and synchronization limits

Neither Samsung's Q900T specifications nor the host observation gives an end-to-end acoustic delay. A2DP is a buffered media transport and includes optional delay reporting, but a report is not an acoustic measurement or a fixed latency guarantee for this speaker. PipeWire can propagate latency values that graph nodes report; those values cannot reveal unreported soundbar DSP or room/speaker arrival time. The actual offset, its change over time, and dropouts therefore need measurement. ([Bluetooth SIG, A2DP v1.4.1, §§2.3 and 5.1.1.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html), [PipeWire latency support](https://docs.pipewire.org/page_latency.html))

Bluetooth's practical advantage is a standard pairing/profile/audio route with a host-visible sink. Its limits are the Q900T's SBC-only Bluetooth codec, buffering/processing delay relative to local output, and no guarantee that two output clocks or acoustic paths stay aligned. A2DP delay reporting can inform diagnostics where both sides implement it; it must not be treated as proof of output alignment. Samsung lists no Bluetooth multi-connection, so plan on the host as the single Bluetooth source. ([Samsung HW-Q900T specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy), [Bluetooth SIG, A2DP v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html))

Samsung publishes no latency number for the Wi-Fi path either. More importantly, the documented Wi-Fi services do not provide a generic Linux output route to time or control. Even if a future protocol investigation finds one, Wi-Fi should only replace A2DP after its sender/receiver controls and end-to-end audio timing are demonstrated with the actual speaker.

## What still needs measurement or device confirmation

1. Recheck the actual unit's model suffix/region and firmware, then confirm BlueZ connection/profile and the negotiated codec on the target host. The prior Audio Sink/PipeWire observation may have changed.
2. Route the same timestamped probe through the local output and Q900T. Use one-clock acoustic capture/cross-correlation at the intended listening position to measure relative arrival, absolute latency, drift, short-term variation, and dropouts. Repeat after Bluetooth reconnect and soundbar restart.
3. Measure audio-to-picture offset separately for video, and event-to-sound/input response for games. A successful music test does not establish acceptable game or video latency.
4. If Wi-Fi remains under consideration, first identify a documented or experimentally verified Linux-compatible receiver protocol for arbitrary host audio, including how playback is started/stopped and how volume and buffering are controlled; then run the same acoustic tests. Until then, do not promise it as a supported path.

## Primary sources

- [Samsung HW-Q900T specifications and features](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy)
- [Samsung HW-Q900T full manual (official PDF)](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_AU&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=7783523&CDCttType=UM&VPath=UM%2F202008%2F20200810142710861%2FAH81-13331R-00_FM_HW-Q900T_XL_XS_XM_XY_XP_ENG_200727.pdf)
- [Bluetooth SIG, Advanced Audio Distribution Profile v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html)
- [BlueZ MediaEndpoint API](https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.MediaEndpoint.rst); [BlueZ A2DP `btmon` documentation](https://raw.githubusercontent.com/bluez/bluez/master/doc/btmon-a2dp.rst)
- [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html); [PipeWire latency support](https://docs.pipewire.org/page_latency.html)
