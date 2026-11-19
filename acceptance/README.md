# Acceptance pipeline

[`multichannel.feature`](multichannel.feature) records observable behavior in the Gherkin subset described by the Acceptance Pipeline Specification (APS). Cargo integration tests and an AgNesse Slim page execute the multichannel CLI acceptance checks. The Gherkin feature is not wired to an APS generator/runtime.

## Executable checks

| Acceptance behavior | Rust check |
| --- | --- |
| Stereo, 5.1, and 7.1 use two, six, and eight channels | `accepts_stereo_and_explicit_51_and_71_layouts` and `stereo_layout_uses_front_left_and_front_right_for_rtp_and_output` |
| Stereo mirror fans out to local and Q900 Bluetooth sinks with independent delay controls | `stereo_mirror_plan_fans_out_to_local_and_bluetooth_outputs`, `mirror_dry_run_shows_two_stereo_outputs_and_independent_delays`, and `mirror_restores_routes_and_stops_both_output_loopbacks_after_sigint` |
| Raw stereo RTP preserves FL/FR and routes to a stereo receiver output, including an optical-capable sink | `sender_plan_sends_stereo_pcm_and_mirrors_to_host_sink`, `receiver_routes_stereo_rtp_to_an_optical_stereo_sink`, `sender_dry_run_sends_stereo_pcm_over_rtp`, `receiver_dry_run_routes_stereo_rtp_to_an_optical_output`, and `loopback.sh stereo` |
| Sender keeps 7.1 as 24-bit PCM and mirrors to the selected sink | `sender_plan_preserves_multichannel_pcm_and_mirrors_to_host_sink` and `sender_dry_run_shows_eight_channel_pcm_plan` |
| Receiver keeps RTP channel order internally and remaps all channels to the selected output | `receiver_plan_selects_pcm_format_layout_and_latency` and the 5.1/7.1 tone identity checks in `loopback.sh` |
| Receiver accepts only matching active output channel maps | `receiver_accepts_71_sink_for_51_and_71`, `receiver_rejects_stereo_sink_for_multichannel_layout`, and `receiver_rejects_missing_channel_positions_even_if_count_is_high_enough` |
| PCM test data isolates stereo, 5.1, and 7.1 channels with 24-bit native-endian samples, silent gaps, and bounded durations; `lpcm-check` streams the tones and removes temporary PipeWire nodes without changing existing routes | `lpcm_channel_tones_are_separate_and_use_24_bit_native_endian_samples`, the duration-boundary acceptance checks, `lpcm_check_dry_run_lists_51_channels_without_changing_default_routes`, `lpcm_check_dry_run_lists_stereo_tones_for_an_optical_output`, `lpcm_check_refuses_71_on_a_six_channel_sink`, and `lpcm_check_plays_native_24_bit_pcm_and_restores_temporary_pipewire_nodes` |
| Pro Audio AUX channels are mapped to HDMI order and require a connected HDMI port on ALSA devices | `receiver_remaps_pro_audio_aux_sink_and_requires_a_connected_hdmi_route` |
| Receiver reads the actual `pw-dump` position format | `parses_pipewire_dump_channel_positions_as_reported_by_pw_dump` |
| The multichannel sender refuses Bluetooth as its uncompressed local mirror; the stereo `mirror` command explicitly routes to Q900 Bluetooth, whose active A2DP codec may be SBC | `sender_refuses_bluetooth_sink_that_may_use_sbc` and the stereo mirror checks above |
| Acoustic observations report fixed skew, drift, jitter, and compensation direction | `reports_fixed_skew_drift_and_compensation_from_acoustic_observations`, `recommends_delaying_q900_when_local_output_arrives_late`, and `sync_report_reads_acoustic_observations_from_csv` |
| Fixed delay is applied to the recommended local or receiver output through PipeWire loopback | `sender_dry_run_shows_eight_channel_pcm_plan`, `receiver_dry_run_applies_fixed_output_delay`, and the sender/receiver Ctrl-C cleanup checks |

Run the Rust checks with `cargo test`; run the no-hardware RTP acceptance path with `bash acceptance/loopback.sh stereo`, `bash acceptance/loopback.sh 5.1`, and `bash acceptance/loopback.sh 7.1`. The script starts the real sender and receiver modules, sends distinct tones through RTP into a simulated output, remaps multichannel AUX channels when applicable, and checks channel identity. Hardware acceptance still needs a receiver connected to the Q900T through an input the soundbar accepts and measured alignment against the local output. HDMI LPCM testing remains open in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12); synchronization tolerances remain open in [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

The AgNesse page starts the Rust Slim fixture and checks sender dry-run output for stereo, six-channel 5.1, eight-channel 7.1, 24-bit PCM, and CLI help. It does not start audio routing or send audio. Build the Rust binaries, then run it against an AgNesse standalone jar:

```sh
cargo build --bins
Q900_BRIDGE_BIN="$PWD/target/debug/q900-bridge" \
  java -jar "$AGNESSE_JAR" -d "$PWD" -r acceptance/agnesse -c 'Q900Bridge?test'
```

`AGNESSE_JAR` should name `agnesse-*-standalone.jar`. The page was executed with AgNesse's standalone runner and all checks passed.

SCRAP only analyzes Speclj specs, so `spec/` contains a small CLI smoke spec that calls the Rust binary through `--dry-run`; product source and runtime remain Rust. Run its assertions with `clojure -M:test`. To assess its structure with SCRAP, set `PROJECT_ROOT` to this repo and `SCRAP_ROOT` to an SCRAP checkout, then run `cd "$SCRAP_ROOT" && clojure -M:scrap "$PROJECT_ROOT/spec"`. Both commands require the Clojure CLI and Speclj dependencies.

The loopback script accepts an optional second argument to exercise an explicit RTP source address, for example `bash acceptance/loopback.sh 5.1 192.168.1.57`. Replace that address with one assigned to the test host's Wi-Fi interface.

## Uncle Bob tools

Mutator, Crapper, and Dryer have direct Rust support and were run against the current implementation; their reports are generated under `.metrics/` and ignored by Git. The changed-source Mutator run killed 105 of 108 covered mutations (97.2%); two survivors only change `Vec` reserve capacity, and one changes a big-endian-only branch that is not compiled on this little-endian host. Dryer found no duplicate candidates in changed sources. SCRAP was run against the Speclj CLI wrapper, and its report does not describe Rust source. A reproducible local setup needs Python 3.11+ and the Rust tool checkouts, with Crapper adjacent to Mutator.

Generate Rust coverage where Crapper searches for LCOV before requesting its coverage report:

```sh
cargo llvm-cov --workspace --all-targets --lcov --output-path target/coverage/rust/lcov.info
crapper --root . --changed --use-existing-coverage
```

SCRAP does not assess Rust source or the Cargo tests. APS's official Go parser and IR-DRY checker were run with Go 1.22 in a temporary container; the generated IR and advisory report are under ignored `target/acceptance/`. The current report found 58 step occurrences, 57 unique steps, and 17 near-duplicate or possible-synonym candidates. They were reviewed and retained because they describe distinct layouts, routes, output timing, or assertions; in particular, the Bluetooth mirror and raw-RTP receiver are separate paths. APS still needs a Rust generator/runtime/step adapter and runner before it can execute or mutate these Gherkin scenarios. Mutator covers Rust source and tests; AgNesse executes the CLI smoke page. No APS Gherkin execution or acceptance-mutation pass is claimed.

To rerun the APS syntax and wording checks, set `APS_ROOT` to a checkout of the official Acceptance Pipeline Specification repository:

```sh
mkdir -p target/acceptance
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-parser /repo/acceptance/multichannel.feature /repo/target/acceptance/multichannel.json
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-ir-dry-checker /repo/target/acceptance/multichannel.json /repo/target/acceptance/multichannel-dry.json
```
