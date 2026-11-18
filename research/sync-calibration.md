# Synchronization and calibration criteria

Research date: 2026-11-19  
Scope: one Linux host sending the same program audio to a local output and a Samsung Q900T over Bluetooth, with music, video, or game audio.

## Decision summary

Classic Bluetooth A2DP does not promise that its output will be synchronized with a separate local output. The profile says there is delay from radio processing, buffering, and codec work, leaves compensation to the implementation, and does not define synchronized point-to-multipoint delivery. A2DP delay reports can help a source estimate the sink path, but source support is optional in the current profile. Therefore, report and calibrate the actual Q900T path; do not treat its configured codec, PipeWire graph latency, or a reported delay as an acoustic measurement. ([Bluetooth SIG, Advanced Audio Distribution Profile v1.4.1, §§2.3 and 5.1.1.2](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html))

The sources reviewed do not establish one universal *audio-to-audio* skew limit for music, video, and games. A useful conservative first-pass target is **no more than 5 ms absolute acoustic skew at a named listening position**. Treat it as a product target to validate, not a standards limit: one two-loudspeaker experiment found a 5.4 ms average echo threshold for an isolated noise-burst pair, but 26.4 ms after repeated burst pairs. The difference shows that stimulus history and listening context matter. Those experiments were not tests of music, a Bluetooth soundbar, or acceptability. ([Yang & Grantham, 1997](https://pubmed.ncbi.nlm.nih.gov/9360483/))

Use these as decision bands while collecting actual measurements:

- **≤5 ms:** conservative candidate target for every content type's audio-to-audio skew.
- **>5 to about 26 ms:** indeterminate; run a user listening comparison with representative content before calling it acceptable.
- **Around or above 26 ms:** high risk of a separately heard echo under some repeated-sound conditions; do not claim universal failure or acceptance from this study alone.

The **5 ms** and **26 ms** boundaries are evidence-informed engineering bands, not guaranteed psychoacoustic cutoffs. The user must choose the release tolerance after seeing device measurements and listening at the intended seat.

## Keep three timing quantities separate

Let `Δ(t) = acoustic arrival at Q900T − acoustic arrival at local output`, measured at the listener. Positive means the Q900T is heard later.

1. **Inter-output skew, Δ:** whether the two copies of audio arrive together.
2. **Absolute audio latency:** time from the program event to either output. Two outputs can have zero skew while both are late.
3. **Audio-to-visual offset:** sound arrival relative to the corresponding visible event. This matters for video and games, but is not an audio-to-audio skew limit.

For television programming, ITU-R BT.1359-1 reports average sound/vision detectability thresholds of about **+45 ms** (sound early) and **−125 ms** (sound late), and average acceptability thresholds of about **+90 ms** and **−185 ms**. Its overall recommended tolerance is +90/−185 ms. These are broadcast sound-versus-picture values; they do not permit 45–185 ms between two speakers. They are also not a game-latency requirement. ([ITU-R BT.1359-1](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1359-1-199811-I%21%21PDF-E.pdf))

Matching a late Bluetooth output by delaying the local output adds that delay to the whole audio mix. This can make an otherwise well-aligned pair late relative to video, or less responsive in a game. A system that only captures and fans out audio cannot make the Q900T render earlier than its Bluetooth path permits; the user must decide whether to accept added whole-system latency, tolerate more inter-output skew, or select another transport/path.

## Reproducible measurement

### 1. Measure acoustic arrival at the listening position

Use a calibrated measurement microphone at a fixed reference seat and a single-clock audio recorder/interface. Route two different, low-level, broadband coded probe sequences (for example, distinct pseudorandom sequences or chirps) through the local and Bluetooth outputs, starting them on the same host audio frame. Record the combined sound, then cross-correlate the recording against each probe to estimate each path's direct-sound arrival. Keep the room quiet and the levels below soundbar limiting/compression; identify the first direct arrival rather than a later room reflection.

At 48 kHz, one recorded sample is about 0.021 ms, comfortably finer than the 5 ms candidate band. The acoustic path, microphone/recorder accuracy, and room reflections dominate the measurement error, so state the recorder and calibration uncertainty with the result. A single microphone and a common ADC clock avoid inter-recorder clock skew. Repeat at the main seat and any other seat the MVP promises to support: the measured acoustic skew includes distance and room effects and can differ by position.

This is an end-to-end acoustic test proposal. PipeWire's stream timing and latency reports are useful diagnostics and starting estimates, not substitutes for this check.

### 2. Separate fixed offset, drift, and jitter

After the Bluetooth link and soundbar DSP have settled, take repeated probe measurements at startup and then at regular intervals during a soak run (a proposed MVP run is 30 minutes). For each time (t), log `Δ(t)` in milliseconds and fit:

`Δ(t) = b + s·t + ε(t)`

- `b` is the fixed relative offset. Compensate by delaying the earlier path by `|b|`.
- `s` is the drift rate. Report it in ms/min and, if useful, ppm. A steady slope indicates a clock-rate mismatch; for a continuous run, compensate with rate matching/asynchronous resampling rather than periodic audible buffer jumps.
- `ε(t)` is short-term variation. Report median, 95th percentile, and maximum absolute residual, plus any dropouts/discontinuities.

Repeat after disconnect/reconnect and any codec, sample-rate, or soundbar-mode change. A one-time fixed offset cannot correct changing transport buffers or clock drift. These intervals and statistics are a proposed test protocol, not published pass limits.

### 3. Measure video/game synchronization independently

For video, use a test clip with a visible flash and a sharp audio event; record screen light with a photodiode and sound with the microphone on one acquisition clock. Report each audio output's offset from the visible event, in addition to `Δ`. For games, instrument a repeatable on-screen event and measure its visual and acoustic arrival during live play; if input-to-response latency matters, capture the physical input event too. Do not infer game responsiveness from the audio-audio skew or from the broadcast tolerance.

## Linux timing and correction

PipeWire exposes graph port latency and can propagate it through the graph, including detection of a latency mismatch between paths. Its stream time report includes queued and buffered samples and delay to the graph edge/device; delay offsets may be used when reported latency is inaccurate. Use these values to initialize and explain a correction, then validate the result acoustically because external Bluetooth/soundbar processing is only represented to the extent it is reported. ([PipeWire latency support](https://docs.pipewire.org/page_latency.html), [PipeWire `pw_time`](https://pipewire.pages.freedesktop.org/pipewire/structpw__time.html))

ALSA exposes queued-frame delay and timestamps. The kernel documentation describes `audio_tstamp` points, including an analog timestamp near the transducer when supported by the hardware. Such timestamps improve the local-device estimate; the exact quality depends on the driver and hardware. ([ALSA PCM timestamping](https://www.kernel.org/doc/html/latest/sound/designs/timestamping.html), [ALSA PCM API](https://alsa.alsa-project.org/alsa-doc/alsa-lib/pcm.html))

PipeWire's audio adapter uses adaptive resampling when a device clock differs from the graph clock. This is a practical mechanism for a measured steady drift rate; it does not establish that a changing A2DP buffer delay is stable or fully reported. ([PipeWire adapter/resampler properties](https://pipewire.pages.freedesktop.org/pipewire/page_man_pipewire-props_7.html), [PipeWire driver clocks](https://docs.pipewire.org/page_driver.html))

## Content-specific acceptance decision

| Content | Audio-to-audio criterion | Separate criterion |
| --- | --- | --- |
| Music | Use the same ≤5 ms candidate; review 5–26 ms by listening. The peer-reviewed thresholds above use noise bursts, so test actual music with clear transients and sustained tones. | No display-sync requirement. User decides whether timbral change/echo at the chosen room and seat is acceptable. |
| Video | Use the same audio-to-audio candidate. | Measure each output against the screen. ITU-R BT.1359-1 is a broadcast reference for this separate measurement, not a speaker-skew target. |
| Games | Use the same audio-to-audio candidate. | Measure visual-event-to-audio timing (and input-to-response if required). No game-specific numeric acceptance value is established by the sources reviewed; set it from measurements and play testing. |

## Values that still need the user's decision

The reference sources cannot supply the Q900T's end-to-end delay, its variability on this host, or a personal acceptability limit. After the measurements, ask the user to choose:

- the target listening seat(s), and whether calibration only promises one seat;
- whether ≤5 ms is required, or a measured value in the review band is acceptable after listening;
- the soak duration and allowable p95/maximum jitter or drift, including behavior after reconnect;
- whether to delay local audio to meet the audio-audio target, accepting the resulting absolute audio latency;
- for video, the preferred sound/vision tolerance; for games, the acceptable event/input latency and whether the feature is useful if the whole mix must be delayed.

No device measurement was available for this research pass, so the bands above are not a claim that the Q900T can meet them.

## Primary sources

- [Bluetooth SIG, Advanced Audio Distribution Profile v1.4.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/a2dp_v1-4-1_1752513648/A2DP_v1.4.1/out/en/index-en.html)
- [Yang & Grantham, “Echo suppression and discrimination suppression aspects of the precedence effect,” *Perception & Psychophysics* (1997)](https://pubmed.ncbi.nlm.nih.gov/9360483/)
- [ITU-R Recommendation BT.1359-1, “Relative timing of sound and vision for broadcasting”](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1359-1-199811-I%21%21PDF-E.pdf)
- [PipeWire latency support](https://docs.pipewire.org/page_latency.html); [PipeWire `pw_time`](https://pipewire.pages.freedesktop.org/pipewire/structpw__time.html); [adapter/resampler properties](https://pipewire.pages.freedesktop.org/pipewire/page_man_pipewire-props_7.html); [driver clocks](https://docs.pipewire.org/page_driver.html)
- [Linux kernel ALSA PCM timestamping](https://www.kernel.org/doc/html/latest/sound/designs/timestamping.html); [ALSA PCM API](https://alsa.alsa-project.org/alsa-doc/alsa-lib/pcm.html)
