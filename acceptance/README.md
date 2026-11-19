# Acceptance pipeline

[`multichannel.feature`](multichannel.feature) records observable stereo and multichannel behavior in the Gherkin subset described by the Acceptance Pipeline Specification (APS). Cargo integration tests and an AgNesse Slim page execute the CLI acceptance checks. The Gherkin feature is not wired to an APS generator/runtime.

## Executable checks

| Acceptance behavior | Rust check |
| --- | --- |
| Stereo, 5.1, and 7.1 use two, six, and eight channels | `accepts_stereo_and_explicit_51_and_71_layouts` and `stereo_layout_uses_front_left_and_front_right_for_rtp_and_output` |
| Stereo mirror fans out to any number of selected PipeWire sinks, with one fixed delay per output and generic transport hints | `stereo_mirror_plan_fans_out_to_any_number_of_outputs_with_independent_delays`, `mirror_dry_run_shows_three_stereo_outputs_and_independent_delays`, `mirror_dry_run_identifies_s_pdif_as_stereo_pcm`, and `mirror_restores_routes_and_stops_every_output_loopback_after_sigint` |
| Raw stereo RTP preserves FL/FR and routes to a stereo receiver output, including an optical-capable sink | `sender_plan_sends_stereo_pcm_and_mirrors_to_host_sink`, `receiver_routes_stereo_rtp_to_an_optical_stereo_sink`, `sender_dry_run_sends_stereo_pcm_over_rtp`, `receiver_dry_run_routes_stereo_rtp_to_an_optical_output`, and `loopback.sh stereo` |
| Sender keeps 7.1 as 24-bit PCM and mirrors to the selected sink | `sender_plan_preserves_multichannel_pcm_and_mirrors_to_host_sink` and `sender_dry_run_shows_eight_channel_pcm_plan` |
| Receiver keeps RTP channel order internally and remaps all channels to the selected output | `receiver_plan_selects_pcm_format_layout_and_latency` and the 5.1/7.1 tone identity checks in `loopback.sh` |
| Receiver accepts only matching active output channel maps | `receiver_accepts_71_sink_for_51_and_71`, `receiver_rejects_stereo_sink_for_multichannel_layout`, and `receiver_rejects_missing_channel_positions_even_if_count_is_high_enough` |
| PCM test data isolates stereo, 5.1, and 7.1 channels with 24-bit native-endian samples, silent gaps, and bounded durations; `lpcm-check` streams the tones and removes temporary PipeWire nodes without changing existing routes | `lpcm_channel_tones_are_separate_and_use_24_bit_native_endian_samples`, the duration-boundary acceptance checks, `lpcm_check_dry_run_lists_51_channels_without_changing_default_routes`, `lpcm_check_dry_run_lists_stereo_tones_for_an_optical_output`, `lpcm_check_refuses_71_on_a_six_channel_sink`, and `lpcm_check_plays_native_24_bit_pcm_and_restores_temporary_pipewire_nodes` |
| Pro Audio AUX channels are mapped to HDMI order and require a connected HDMI port on ALSA devices | `receiver_remaps_pro_audio_aux_sink_and_requires_a_connected_hdmi_route` |
| Receiver reads the actual `pw-dump` position format | `parses_pipewire_dump_channel_positions_as_reported_by_pw_dump` |
| The multichannel sender refuses Bluetooth as its uncompressed local mirror; the stereo `mirror` command identifies Bluetooth A2DP as possibly SBC and supports a stereo PCM S/PDIF target | `sender_refuses_bluetooth_sink_that_may_use_sbc`, `mirror_dry_run_shows_three_stereo_outputs_and_independent_delays`, and `mirror_dry_run_identifies_s_pdif_as_stereo_pcm` |
| Acoustic observations report fixed skew, drift, jitter, and compensation direction | `reports_fixed_skew_drift_and_compensation_from_acoustic_observations`, `recommends_delaying_q900_when_local_output_arrives_late`, and `sync_report_reads_acoustic_observations_from_csv` |
| Fixed delay is applied to the recommended local or receiver output through PipeWire loopback | `sender_dry_run_shows_eight_channel_pcm_plan`, `receiver_dry_run_applies_fixed_output_delay`, and the sender/receiver Ctrl-C cleanup checks |
| The local Rust/WASM web panel validates microphone/output selections and per-output delays; preview uses `mirror --dry-run` before explicit start | `web_rejects_invalid_ports_before_starting_the_server`, `web_port_defaults_and_accepts_both_u16_boundaries`, `web_port_rejects_zero_overflow_and_non_numeric_values`, `settings_accept_available_microphone_and_two_stereo_outputs`, `settings_reject_unavailable_devices_duplicate_sinks_and_excessive_delay`, `one_stereo_output_is_valid_for_settings_but_not_for_mirroring`, and the three `calibration_report_*` settings-match tests |
| Calibration dry-run maps a distinct probe channel to each selected sink; signal analysis covers exact probe-window boundaries and stereo channel indexing, tolerates broadband noise and a distinct weak probe at the HDMI screenshot's 0.162 correlation, rejects noise-only, silence, clipping, ambiguous peaks, and low-quality results, and proposes only non-negative compensation from repeated measurements | `calibration_dry_run_lists_the_microphone_and_parallel_output_routes`, `estimates_the_known_arrival_of_a_scaled_probe`, `finds_a_minimum_length_probe_on_the_right_channel_at_the_last_window`, `generated_probe_is_repeatable_distinct_and_limited_to_ten_percent`, `identifies_distinct_output_probes_in_one_recording`, `estimates_probe_arrival_under_broadband_capture_noise`, `estimates_a_distinct_probe_with_the_weak_correlation_seen_on_hdmi`, `random_capture_noise_without_the_probe_is_rejected`, `rejects_silence_and_clipped_probe_recordings`, `rejects_two_equally_strong_probe_arrivals_as_ambiguous`, `proposes_only_non_negative_offsets_relative_to_the_slowest_path`, `delay_proposals_accept_zero_and_two_seconds_but_reject_negative_or_larger_values`, `calibration_plan_sends_distinct_probe_channels_to_the_selected_outputs`, `calibration_plan_accepts_sixteen_outputs_and_rejects_a_seventeenth`, `calibration_report_keeps_repeats_and_uses_the_slowest_path_as_reference`, `build_report_rejects_a_measurement_below_either_quality_threshold`, and `build_report_rejects_non_finite_or_negative_measurements` |

Run the Rust checks with `cargo test --workspace`; run the no-hardware RTP acceptance path with `bash acceptance/loopback.sh stereo`, `bash acceptance/loopback.sh 5.1`, and `bash acceptance/loopback.sh 7.1`. The script starts the real sender and receiver modules, sends distinct tones through RTP into a simulated output, remaps multichannel AUX channels when applicable, and checks channel identity. The user reports that the host's optical connector is not connected to the Q900T, so the direct stereo optical route still needs an audible hardware check. The wireless RTP route needs a second Linux receiver with an output accepted by the soundbar. Synthetic calibration analysis is implemented, but a complete repeated hardware measurement and persistent calibration profile are still open; the user reports a microphone about one metre above the soundbar. HDMI LPCM testing remains open in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12); implementation is tracked in [issue #15](https://github.com/guility/samsung-q900-wifi-linux/issues/15) and synchronization criteria remain open in [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

The AgNesse page starts the Rust Slim fixture and checks sender dry-run output for stereo, six-channel 5.1, eight-channel 7.1, 24-bit PCM, and CLI help, including the repeated generic output option. It does not start audio routing or send audio. Build the Rust binaries, then run it against an AgNesse standalone jar:

```sh
cargo build --bins
Q900_BRIDGE_BIN="$PWD/target/debug/q900-bridge" \
  java -jar "$AGNESSE_JAR" -d "$PWD" -r acceptance/agnesse -c 'Q900Bridge?test'
```

`AGNESSE_JAR` should name `agnesse-*-standalone.jar`. The page was executed with AgNesse's standalone runner and all 8 checks passed.

SCRAP only analyzes Speclj specs, so `spec/` contains a small CLI smoke spec that calls the Rust binary through `--dry-run`; product source and runtime remain Rust. Run its assertions with the SCRAP checkout's `:spec` alias (`clojure -M:spec`), exposing this repository's `spec/` directory as the checkout's `spec/` path and setting `Q900_BRIDGE_BIN` to the built Rust executable. To assess its structure with SCRAP, set `PROJECT_ROOT` to this repo and `SCRAP_ROOT` to an SCRAP checkout, then run `cd "$SCRAP_ROOT" && clojure -M:scrap "$PROJECT_ROOT/spec"`. Both commands require the Clojure CLI and Speclj dependencies.

The loopback script accepts an optional second argument to exercise an explicit RTP source address, for example `bash acceptance/loopback.sh 5.1 192.168.1.57`. Replace that address with one assigned to the test host's Wi-Fi interface.

## Uncle Bob tools

Mutator, Crapper, and Dryer have direct Rust support and their reports are generated under `.metrics/` and ignored by Git. The latest incremental Mutator run for this calibration change selected 128 covered sites: tests killed 81 mutations and 47 survived (63.3%); no selected sites were uncovered. Survivors include arithmetic and boundary mutations in correlation, probe generation, and report validation; this is not a claim of complete mutation coverage. Crapper reports `estimate_arrival` at 90.2% coverage and CRAP 13.2, and `build_report` at 92.6% and CRAP 18.1. The hardware capture functions remain untested by automated tests, and the Yew `App` and its event handlers have no coverage data; WASM compilation and release bundling are verified separately. Dryer reports one possible 0.83 similarity between two state methods that lock different fields; the exact duplicated constructors and web request bodies were factored into shared helpers. SCRAP assessed the Speclj CLI wrapper as stable (score 12.1); its 2 examples and 18 assertions passed, but its report does not describe Rust source. A reproducible local setup needs Python 3.11+ and the Rust tool checkouts, with Crapper adjacent to Mutator.

Generate Rust coverage where Crapper searches for LCOV before requesting its coverage report:

```sh
cargo llvm-cov --workspace --all-targets --lcov --output-path target/coverage/rust/lcov.info
crapper --root . --changed --use-existing-coverage
```

SCRAP does not assess Rust source or the Cargo tests. APS's official Go parser and IR-DRY checker were run with Go 1.22 in a temporary container; the generated IR and advisory report are under ignored `target/acceptance/`. The latest report found 81 step occurrences, 75 unique steps, and 20 near-duplicate or possible-synonym candidates. They were reviewed and retained because they describe distinct layouts, Bluetooth/optical/RTP routes, output timing, or assertions. The Bluetooth and optical mirror scenarios share wording where appropriate but exercise different transports. APS still needs a Rust generator/runtime/step adapter and runner before it can execute or mutate these Gherkin scenarios. Mutator covers Rust source and tests; AgNesse executes the CLI smoke page. No APS Gherkin execution or acceptance-mutation pass is claimed.

To rerun the APS syntax and wording checks, set `APS_ROOT` to a checkout of the official Acceptance Pipeline Specification repository:

```sh
mkdir -p target/acceptance
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-parser /repo/acceptance/multichannel.feature /repo/target/acceptance/multichannel.json
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-ir-dry-checker /repo/target/acceptance/multichannel.json /repo/target/acceptance/multichannel-dry.json
```
