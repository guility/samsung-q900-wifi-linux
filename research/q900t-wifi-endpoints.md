# Q900T Wi-Fi endpoint: Spotify Connect and host discovery

**Checked:** 2026-11-19
**Scope:** Supplemental findings for the existing Q900T wireless-input research. This note records the Spotify Connect behavior documented by Spotify and the supplied host's network-discovery results; the broader interface and multichannel findings are covered by the existing report on branch `research/q900t-wireless-inputs` (`research/q900t-direct-wireless-and-hdmi-lpcm.md`).

## Spotify Connect is a Spotify playback path

Samsung lists Spotify Connect as a feature of the HW-Q900T. Spotify's support guide explains that Connect lets one device remotely control listening on another: on first connection the devices should share Wi-Fi, then the user starts something in Spotify and picks the playback device. Spotify's developer guide is more explicit: selecting a wireless speaker transfers Spotify playback to it, while the phone remains a remote for pause, skip, and volume; the speaker's Spotify Connect application receives and plays the audio data.

**Inference:** these documents describe a Spotify-content playback/control session. They do not describe Spotify Connect as an input for arbitrary PCM captured from all desktop applications, so the Q900T's Spotify Connect feature is not documented as a Linux system-audio sink. The sources also do not specify multichannel channel mapping or real-time latency for the Q900T's Spotify Connect path.

## Supplied host discovery results

The current LAN checks found:

- No `_spotify-connect._tcp` service and no Q900T mDNS service in `avahi-browse` results.
- The only AirPlay-related mDNS service was an Apple TV 3,2 advertising `_raop._tcp`; it was not the Q900T.
- SSDP M-SEARCH returned the router at `192.168.1.1`, with no Q900T response.

This is a runtime observation for the current network and device state, not a guarantee that no undocumented or inactive protocol exists. It does mean there is no discovered Q900T service endpoint for the Linux host to target at the time of the checks.

## Sources

- [Samsung HW-Q900T product specifications](https://www.samsung.com/au/audio-devices/soundbar/7-1-2-ch-soundbar-dolby-atmos-black-hw-q900t-xy/) — lists Spotify Connect for the Q900T.
- [Spotify Support: Spotify Connect](https://support.spotify.com/us/article/spotify-connect/) — describes device-to-device playback control, starting playback in Spotify, selecting a device, and same-Wi-Fi setup.
- [Spotify for Developers: Spotify Connect Basics](https://developer.spotify.com/documentation/commercial-hardware/implementation/guides/connect-basics) — describes transferring Spotify playback to a wireless speaker, the app as remote control, and the speaker app receiving audio data for playback.
