# Acceptance pipeline

[`multichannel.feature`](multichannel.feature) records observable behavior in the Gherkin subset described by the Acceptance Pipeline Specification (APS). The executable Rust checks currently live in Cargo integration tests; this feature file is not yet wired to an APS generator/runtime.

## Executable checks

| Acceptance behavior | Rust check |
| --- | --- |
| 5.1 and 7.1 use six and eight channels | `accepts_only_explicit_51_and_71_layouts` |
| Sender keeps 7.1 as 24-bit PCM and mirrors to the selected sink | `sender_plan_preserves_multichannel_pcm_and_mirrors_to_host_sink` and `sender_dry_run_shows_eight_channel_pcm_plan` |
| Receiver keeps RTP channel order internally and remaps all channels to the selected output | `receiver_plan_selects_pcm_format_layout_and_latency` and the 5.1/7.1 tone identity checks in `loopback.sh` |
| Receiver accepts only matching active output channel maps | `receiver_accepts_71_sink_for_51_and_71`, `receiver_rejects_stereo_sink_for_multichannel_layout`, and `receiver_rejects_missing_channel_positions_even_if_count_is_high_enough` |
| Receiver reads the actual `pw-dump` position format | `parses_pipewire_dump_channel_positions_as_reported_by_pw_dump` |
| Bluetooth/SBC is refused as the local mirror | `sender_refuses_bluetooth_sink_that_may_use_sbc` |

Run the Rust checks with `cargo test`; run the no-hardware RTP acceptance path with `bash acceptance/loopback.sh 5.1` and `bash acceptance/loopback.sh 7.1`. The script starts the real sender and receiver modules, sends distinct tones through RTP, remaps channel positions onto a simulated output, and checks channel identity. Hardware acceptance still needs a receiver with HDMI LPCM to the Q900T, a channel-identification signal for 5.1 and 7.1, and measured alignment against the local output. Those checks remain open in [issue #9](https://github.com/guility/samsung-q900-wifi-linux/issues/9) and [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

## Uncle Bob tools

Mutator, Crapper, and Dryer have direct Rust support and were run against the current implementation. Their reports are generated under `.metrics/` and ignored by Git. A reproducible local setup needs Python 3.11+ and the three checkouts, with Crapper adjacent to Mutator.

SCRAP analyzes Speclj/Clojure specs rather than Rust or Gherkin, so it does not assess the Cargo tests. AgNesse needs a separate Rust Slim fixture process. APS needs a Rust generator/runtime/step adapter before it can execute the feature file. Those adapters are not part of this transport prototype, and the related tools are not represented as passing checks.
