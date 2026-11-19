# Acceptance pipeline

[`multichannel.feature`](multichannel.feature) records observable behavior in the Gherkin subset described by the Acceptance Pipeline Specification (APS). The executable Rust checks currently live in Cargo integration tests; this feature file is not yet wired to an APS generator/runtime.

## Executable checks

| Acceptance behavior | Rust check |
| --- | --- |
| 5.1 and 7.1 use six and eight channels | `accepts_only_explicit_51_and_71_layouts` |
| Sender keeps 7.1 as 24-bit PCM and mirrors to the selected sink | `sender_plan_preserves_multichannel_pcm_and_mirrors_to_host_sink` and `sender_dry_run_shows_eight_channel_pcm_plan` |
| Receiver keeps RTP channel order internally and remaps all channels to the selected output | `receiver_plan_selects_pcm_format_layout_and_latency` and the 5.1/7.1 tone identity checks in `loopback.sh` |
| Receiver accepts only matching active output channel maps | `receiver_accepts_71_sink_for_51_and_71`, `receiver_rejects_stereo_sink_for_multichannel_layout`, and `receiver_rejects_missing_channel_positions_even_if_count_is_high_enough` |
| LPCM test data isolates every 5.1/7.1 channel with 24-bit native-endian samples, silent gaps, and bounded durations; `lpcm-check` streams the tones and removes temporary PipeWire nodes without changing existing routes | `lpcm_channel_tones_are_separate_and_use_24_bit_native_endian_samples`, the duration-boundary acceptance checks, `lpcm_check_dry_run_lists_51_channels_without_changing_default_routes`, `lpcm_check_refuses_71_on_a_six_channel_sink`, and `lpcm_check_plays_native_24_bit_pcm_and_restores_temporary_pipewire_nodes` |
| Pro Audio AUX channels are mapped to HDMI order and require a connected HDMI port on ALSA devices | `receiver_remaps_pro_audio_aux_sink_and_requires_a_connected_hdmi_route` |
| Receiver reads the actual `pw-dump` position format | `parses_pipewire_dump_channel_positions_as_reported_by_pw_dump` |
| Bluetooth/SBC is refused as the local mirror | `sender_refuses_bluetooth_sink_that_may_use_sbc` |
| Acoustic observations report fixed skew, drift, jitter, and compensation direction | `reports_fixed_skew_drift_and_compensation_from_acoustic_observations`, `recommends_delaying_q900_when_local_output_arrives_late`, and `sync_report_reads_acoustic_observations_from_csv` |
| Fixed delay is applied to the recommended local or receiver output through PipeWire loopback | `sender_dry_run_shows_eight_channel_pcm_plan`, `receiver_dry_run_applies_fixed_output_delay`, and the sender/receiver Ctrl-C cleanup checks |

Run the Rust checks with `cargo test`; run the no-hardware RTP acceptance path with `bash acceptance/loopback.sh 5.1` and `bash acceptance/loopback.sh 7.1`. The script starts the real sender and receiver modules, sends distinct tones through RTP into a simulated AUX output, remaps them to HDMI channel order, and checks channel identity. Hardware acceptance still needs a receiver with HDMI LPCM to the Q900T, a channel-identification signal for 5.1 and 7.1, and measured alignment against the local output. Those checks remain open in [issue #12](https://github.com/guility/samsung-q900-wifi-linux/issues/12) and [issue #2](https://github.com/guility/samsung-q900-wifi-linux/issues/2).

The loopback script accepts an optional second argument to exercise an explicit RTP source address, for example `bash acceptance/loopback.sh 5.1 192.168.1.57`. Replace that address with one assigned to the test host's Wi-Fi interface.

## Uncle Bob tools

Mutator, Crapper, and Dryer have direct Rust support and were run against the current implementation. Their reports are generated under `.metrics/` and ignored by Git. A reproducible local setup needs Python 3.11+ and the three checkouts, with Crapper adjacent to Mutator.

Generate Rust coverage where Crapper searches for LCOV before requesting its coverage report:

```sh
cargo llvm-cov --workspace --all-targets --lcov --output-path target/coverage/rust/lcov.info
crapper --root . --changed --use-existing-coverage
```

SCRAP analyzes Speclj/Clojure specs rather than Rust or Gherkin, so it does not assess the Cargo tests. APS's official Go parser and IR-DRY checker were run with Go 1.22 in a temporary container; the generated IR and advisory report are under ignored `target/acceptance/`. The report found 46 step occurrences, 45 unique steps, and 14 near-duplicate or possible-synonym candidates. They were reviewed and retained because they describe distinct layouts, routes, or assertions. APS still needs a Rust generator/runtime/step adapter and runner before it can execute or mutate these scenarios. AgNesse needs a separate Rust Slim fixture process. These adapters are not part of this transport prototype, and no APS/AgNesse execution or acceptance-mutation pass is claimed.

To rerun the APS syntax and wording checks, set `APS_ROOT` to a checkout of the official Acceptance Pipeline Specification repository:

```sh
mkdir -p target/acceptance
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-parser /repo/acceptance/multichannel.feature /repo/target/acceptance/multichannel.json
docker run --rm -v "$APS_ROOT:/aps:ro" -v "$PWD:/repo" -w /aps golang:1.22 \
  go run ./cmd/gherkin-ir-dry-checker /repo/target/acceptance/multichannel.json /repo/target/acceptance/multichannel-dry.json
```
