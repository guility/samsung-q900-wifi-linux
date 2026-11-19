# Q900T direct wireless input and HDMI LPCM

Research date: 2026-11-19

## Finding

**There is no documented direct generic Linux Wi-Fi or Bluetooth path that delivers above-SBC 5.1/7.1 audio to the HW-Q900T.** Samsung lists the Q900T Bluetooth codec as SBC; its manual describes Bluetooth listening as stereo and says the bar supports SBC at 44.1/48 kHz over A2DP. Samsung's manual describes Wi-Fi audio connection to a TV as available only with Samsung TVs. The product also lists Spotify Connect, but neither Samsung source describes that service feature as a general network PCM input or 5.1/7.1 channel-mapped input. This is a conclusion about documented interfaces; it does not prove that no undocumented protocol exists.

**The supported candidate path still requiring a device test is raw PCM over Wi-Fi/RTP to a Linux receiver, then uncompressed multichannel LPCM over HDMI into the Q900T.** Samsung documents the HDMI connections and Dolby/DTS formats, but the manual does not state whether HDMI accepts 5.1/7.1 LPCM. Its 7.1.2 speaker layout and supported Dolby/DTS formats do not establish LPCM support. Do not treat the HDMI leg as verified until the direct hardware check below passes.

## Documented facts

| Interface | What Samsung documents | What that does and does not establish |
| --- | --- | --- |
| Bluetooth | The model page lists Bluetooth codec **SBC**. The manual says Bluetooth playback is stereo, supports SBC at 44.1 and 48 kHz, and requires A2DP. | Samsung documents no higher Bluetooth codec or multichannel Bluetooth input for the Q900T. Linux cannot make the receiver accept an unadvertised codec merely by choosing it. |
| Wi-Fi | The manual says Wi-Fi connection to a TV is available only on a Samsung TV. Samsung lists Wi-Fi and Spotify Connect on the model page. | Neither source documents generic RTP/network PCM reception or multichannel Wi-Fi input. Wi-Fi presence and Spotify Connect do not show that the Q900T is a generic Linux network audio sink. |
| HDMI | The manual shows an external device's HDMI output connected to HDMI IN 1 or 2, with HDMI TO TV (eARC/ARC) connected to the TV. It lists Dolby Atmos (TrueHD/Digital Plus), Dolby Digital, DTS-HD Master Audio, DTS:X, and DTS as supported play formats. | The manual does not give an HDMI LPCM input format table, LPCM channel count, or LPCM sample limits. Acceptance of discrete 5.1/7.1 LPCM remains unknown. |

Samsung's product page lists a 7.1.2-channel speaker system and two HDMI inputs, but speaker count is not an input-format guarantee. Samsung's spec sheet and manual list Dolby/DTS formats; those decoder entries do not establish that HDMI accepts multichannel LPCM.

## Standards context and inference

The Bluetooth SIG A2DP profile allows audio distribution in mono, stereo, or multichannel modes. It requires SBC interoperability and defines other codec interoperability requirements as optional. That is a profile-level capability, not a promise that every A2DP receiver supports every codec or channel layout. The Q900T-specific Samsung documentation names SBC and stereo, so the general A2DP profile cannot be used to infer above-SBC or 5.1/7.1 support on this soundbar.

Samsung's Wi-Fi documentation is specifically about TV-to-soundbar connection and service features. **Inference:** it does not provide a documented protocol for a Linux machine to send arbitrary PCM directly to the bar. A Linux RTP sender would need a receiver endpoint; the Q900T's Wi-Fi feature list does not document one.

## Exact hardware check still required

1. Use a Linux HDMI source with known 6- and 8-channel LPCM output. Connect its HDMI output **directly to Q900T HDMI IN 1 or HDMI IN 2**. Connect the soundbar's **HDMI TO TV (eARC/ARC)** output to a display if needed for normal setup. Select the matching HDMI input on the soundbar.
2. Inspect the Linux HDMI sink's EDID/ELD and ALSA/PipeWire capabilities with the Q900T connected. Record whether it advertises LPCM, channel counts, supported sample rates, and sample sizes. A generic 5.1/7.1 profile without a connected Q900T ELD is not proof.
3. Send discrete uncompressed 48 kHz LPCM channel-identification signals, first as 5.1 (6 channels) and then 7.1 (8 channels). Verify that both modes produce sound, that each tone reaches the intended channel, and that output is not silently downmixed, rejected, or remapped. Record exact channel order, format, and rate.
4. Repeat the test with the actual candidate Linux receiver that would receive RTP. A pass from a separate test host proves Q900T input acceptance only; it does not prove that the candidate receiver can negotiate and output 6/8-channel LPCM.
5. Only after direct HDMI playback passes, connect the receiver to the existing RTP/Wi-Fi sender and measure dropouts, channel mapping, latency, and drift during simultaneous local playback.

No hardware was purchased or provisioned for this research. If the Q900T HDMI input rejects or downmixes multichannel LPCM, this report does not establish an alternative live Dolby/DTS encoder path from the Linux RTP PCM stream.

## Sources

- [Samsung HW-Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/) — Bluetooth codec SBC, Wi-Fi, Spotify Connect, HDMI input/output, and model features.
- [Samsung HW-Q900T support page](https://www.samsung.com/uk/support/model/HW-Q900T/ZF/) — official listing for the current English manual.
- [Samsung HW-Q900T User Manual, version 2402130 (PDF)](https://org.downloadcenter.samsung.com/downloadfile/ContentsFile.aspx?CDSite=UNI_UK&OriginYN=N&ModelType=N&ModelName=HW-Q900T&CttFileID=9492842&CDCttType=UM&VPath=UM/202402/20240215110810001/AH81-13331K-02_FM_HW-Q900T_XU_ENG_240213.0.pdf) — printed pp. 26–27 (Samsung-TV-only Wi-Fi TV connection; external HDMI setup), pp. 29–30 (stereo Bluetooth, A2DP, SBC 44.1/48 kHz), and p. 45 (supported play formats).
- [Bluetooth SIG, Advanced Audio Distribution Profile (A2DP) v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html) — profile scope, mandatory SBC, and optional codec interoperability requirements.
- [Samsung HW-Q900T/ZA specification sheet (PDF)](https://image-us.samsung.com/SamsungUS/samsung/us/tvs/HW-Q900T_Soundbar_Spec_Sheet_Final_Files.pdf) — 7.1.2-channel system, HDMI connections, and listed Dolby/DTS formats.

## Decision

Do not plan to use the Q900T as a generic Wi-Fi receiver or as an above-SBC 5.1/7.1 Bluetooth target. Keep the Linux Wi-Fi/RTP-to-HDMI receiver architecture conditional on a successful 5.1 and 7.1 LPCM test through the Q900T's own HDMI input.
