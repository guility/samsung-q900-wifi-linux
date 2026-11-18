# Q900T wireless path for higher-quality 5.1/7.1 from Linux

Research date: 2026-11-19

## Finding

No documented, generic wireless Linux-to-HW-Q900T path is verified to meet both requirements: quality above SBC and transmission of 5.1/7.1 channels.

The host can connect to the Q900T over Bluetooth A2DP, but Samsung lists SBC as the soundbar's Bluetooth codec. The Bluetooth SIG's SBC channel modes are mono, dual-channel, stereo, and joint stereo, so this is at most a two-channel input. PipeWire also advertises an SBC-XQ profile; that is a higher-bitrate mode within SBC, not a different codec family, and it remains stereo. It does not meet the stated requirement. ([Samsung Q900T specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/), [Bluetooth SIG A2DP v1.4.1, §4.3](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html), [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html))

The Q900T's Wi-Fi TV Connection is a TV-specific route. Samsung says it works with select Samsung TVs; the 2024 Q900T manual describes selecting the soundbar from the Samsung TV's Sound Output menu on the same Wi-Fi network. The manual gives no Wi-Fi codec, bitrate, quality, or channel count, and says Dolby Atmos/DTS:X are supported in HDMI mode only. The host's connected display is a TCL device named “Beyond TV,” so this Samsung-TV relay is unavailable on the current setup. Samsung does not document the Q900T as a general Linux Wi-Fi audio renderer. ([2024 Q900T manual](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf), [Samsung Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf), [Samsung Wi-Fi TV connection guide](https://www.samsung.com/uk/support/tv-audio-video/how-to-connect-a-soundbar-to-the-tv-via-wi-fi-network/))

The 7.1.2 figure describes the soundbar's speaker layout. It does not establish the number of channels it receives over Wi-Fi or Bluetooth.

## Higher-quality input path

Samsung documents two HDMI inputs and high-quality formats including Dolby Atmos/TrueHD, Dolby Digital Plus, DTS:X, and DTS 5.1. The 2024 manual's output-mode table includes 5.1- and 7.1-channel inputs, but it does not identify the input transport or confirm 5.1/7.1 LPCM. I found no HDMI audio-format table or LPCM entry in that manual, so multichannel LPCM support remains unverified. The documented direct high-quality route is HDMI with a supported Dolby/DTS bitstream; HDMI eARC is the return-audio path for a compatible TV. ([Samsung Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/), [Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf), [2024 Q900T manual](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf))

## Linux implications

The current host exposes a Bluetooth A2DP/SBC sink for the Q900T. PipeWire also lists SBC-XQ as an available profile; it remains SBC and does not add channels. BlueZ and PipeWire can offer other codecs when both endpoints support them, but the published Q900T model specification lists SBC only. ([BlueZ MediaEndpoint API](https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.MediaEndpoint.rst), [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html))

A wireless host-to-soundbar design that preserves multichannel audio would need a separate receiver near the Q900T: the host would send audio over a documented network protocol, and that receiver would output a Q900T-supported multichannel format over HDMI. That adds hardware and receiver software. Before committing to LPCM, verify that the actual Q900T accepts 5.1/7.1 LPCM over its HDMI input; the manual does not settle that. Alternatively, a direct wired HDMI source can send a documented supported Dolby/DTS bitstream, subject to confirming the host and Q900T format negotiation.

A preliminary microphone capture was not stable enough to count as an acceptance measurement. No calibrated acoustic measurement at the intended listening position has been completed. After selecting a viable path, measure channel format, output skew, latency, drift, jitter, and dropouts.

## Recommendation

Do not use Bluetooth A2DP/SBC for the requested output. No built-in Q900T wireless route is currently verified to meet the above-SBC and 5.1/7.1 requirements. The next design choice is whether to use wired HDMI or add a Wi-Fi-to-HDMI receiver; the receiver's output format and latency must be validated before implementation.

## Sources

- [Samsung HW-Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/)
- [Samsung HW-Q900T specification sheet](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf)
- [Samsung HW-Q900T manual, version 2402130](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM%2F202402%2F20240215110810001%2FAH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf)
- [Samsung Wi-Fi soundbar/TV connection guide](https://www.samsung.com/uk/support/tv-audio-video/how-to-connect-a-soundbar-to-the-tv-via-wi-fi-network/)
- [Bluetooth SIG, Advanced Audio Distribution Profile v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html)
- [BlueZ MediaEndpoint API](https://raw.githubusercontent.com/bluez/bluez/master/doc/org.bluez.MediaEndpoint.rst)
- [WirePlumber Bluetooth configuration](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/bluetooth.html)
