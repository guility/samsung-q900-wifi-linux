use q900_bridge::calibration::{ArrivalEstimate, estimate_arrival};

#[test]
fn estimates_the_known_arrival_of_a_scaled_probe() {
    let probe: Vec<f32> = (0..240)
        .map(|index| (((index * 37 + 13) % 101) as f32 / 50.0) - 1.0)
        .collect();
    let expected_offset = 337;
    let mut recording = vec![0.0; 2400];
    for (index, sample) in probe.iter().enumerate() {
        recording[expected_offset + index] = sample * 0.35;
    }

    let estimate = estimate_arrival(&probe, &recording, 1, 8_000).unwrap();

    assert_eq!(estimate.sample_offset, expected_offset);
    assert!((estimate.arrival_ms - 42.125).abs() < 0.001);
    assert!(estimate.confidence > 0.99);
    assert!(estimate.peak_to_sidelobe_ratio > 1.15);
}

#[test]
fn finds_a_minimum_length_probe_on_the_right_channel_at_the_last_window() {
    let mut state = 0x1234_5678_u32;
    let probe: Vec<_> = (0..80)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            if state & 1 == 0 { -1.0 } else { 1.0 }
        })
        .collect();
    let offset = 120;
    let frames = offset + probe.len();
    let mut recording = vec![0.0; frames * 2];
    for (index, sample) in probe.iter().enumerate() {
        recording[(offset + index) * 2 + 1] = sample * 0.35;
    }

    let estimate = estimate_arrival(&probe, &recording, 2, 8_000).unwrap();

    assert_eq!(estimate.sample_offset, offset);
    assert_eq!(estimate.arrival_ms, 15.0);
    assert_eq!(estimate.channel_index, 1);
    assert!(estimate.confidence > 0.99);
}

#[test]
fn generated_probe_is_repeatable_distinct_and_limited_to_ten_percent() {
    use q900_bridge::calibration::generate_probe;

    let probe = generate_probe(48_000, 42).unwrap();

    assert_eq!(probe.len(), 1023 * 9);
    assert_eq!(probe.first(), Some(&0.0));
    assert_eq!(probe.last(), Some(&0.0));
    assert!(probe.iter().copied().fold(f32::NEG_INFINITY, f32::max) <= 0.1);
    assert!(probe.iter().copied().fold(f32::INFINITY, f32::min) < -0.09);
    assert_eq!(probe, generate_probe(48_000, 42).unwrap());
    assert_ne!(probe, generate_probe(48_000, 43).unwrap());
}

#[test]
fn proposes_only_non_negative_offsets_relative_to_the_slowest_path() {
    use q900_bridge::calibration::{PathArrival, propose_delays};

    let arrivals = [
        PathArrival {
            sink: "hdmi".to_owned(),
            arrivals_ms: vec![100.0, 101.0, 99.0],
        },
        PathArrival {
            sink: "q900t".to_owned(),
            arrivals_ms: vec![224.0, 223.0, 225.0],
        },
    ];

    let proposals = propose_delays(&arrivals).unwrap();

    assert_eq!(proposals[0].sink, "hdmi");
    assert_eq!(proposals[0].median_arrival_ms, 100.0);
    assert_eq!(proposals[0].delay_ms, 124);
    assert_eq!(proposals[1].sink, "q900t");
    assert_eq!(proposals[1].median_arrival_ms, 224.0);
    assert_eq!(proposals[1].delay_ms, 0);
}

#[test]
fn delay_proposals_accept_zero_and_two_seconds_but_reject_negative_or_larger_values() {
    use q900_bridge::calibration::{PathArrival, propose_delays};

    let arrivals = [
        PathArrival {
            sink: "early".to_owned(),
            arrivals_ms: vec![0.0, 0.0, 0.0, 0.0],
        },
        PathArrival {
            sink: "late".to_owned(),
            arrivals_ms: vec![2_000.0, 2_000.0, 2_000.0, 2_000.0],
        },
    ];
    let proposals = propose_delays(&arrivals).unwrap();
    assert_eq!(proposals[0].delay_ms, 2_000);
    assert_eq!(proposals[1].delay_ms, 0);
    assert_eq!(proposals[0].median_arrival_ms, 0.0);

    let negative = [
        PathArrival {
            sink: "negative".to_owned(),
            arrivals_ms: vec![-1.0, 0.0, 1.0],
        },
        PathArrival {
            sink: "other".to_owned(),
            arrivals_ms: vec![10.0, 10.0, 10.0],
        },
    ];
    assert!(propose_delays(&negative).is_err());

    let too_late = [
        PathArrival {
            sink: "early".to_owned(),
            arrivals_ms: vec![0.0, 0.0, 0.0],
        },
        PathArrival {
            sink: "late".to_owned(),
            arrivals_ms: vec![2_001.0, 2_001.0, 2_001.0],
        },
    ];
    assert!(propose_delays(&too_late).is_err());
}

#[test]
fn identifies_distinct_output_probes_in_one_recording() {
    use q900_bridge::calibration::generate_probe;

    let hdmi_probe = generate_probe(48_000, 0).unwrap();
    let bluetooth_probe = generate_probe(48_000, 1).unwrap();
    let hdmi_offset = 4_800;
    let bluetooth_offset = 10_200;
    let mut recording = vec![0.0; 48_000 * 2];
    for (index, sample) in hdmi_probe.iter().enumerate() {
        recording[hdmi_offset + index] += sample;
    }
    for (index, sample) in bluetooth_probe.iter().enumerate() {
        recording[bluetooth_offset + index] += sample;
    }

    let hdmi = estimate_arrival(&hdmi_probe, &recording, 1, 48_000).unwrap();
    let bluetooth = estimate_arrival(&bluetooth_probe, &recording, 1, 48_000).unwrap();

    assert!(hdmi.sample_offset.abs_diff(hdmi_offset) <= 3);
    assert!(bluetooth.sample_offset.abs_diff(bluetooth_offset) <= 3);
}

#[test]
fn calibration_plan_sends_distinct_probe_channels_to_the_selected_outputs() {
    use q900_bridge::calibration::calibration_plan;

    let outputs = vec!["hdmi-sink".to_owned(), "q900t-sink".to_owned()];
    let plan = calibration_plan("brio-source", &outputs, 1234).unwrap();

    assert_eq!(plan.microphone, "brio-source");
    assert_eq!(plan.bus_sink, "q900_calibration_1234");
    assert_eq!(plan.modules.len(), 5);
    assert!(plan.modules[0].args.contains(&"channels=2".to_owned()));
    assert!(
        plan.modules[0]
            .args
            .contains(&"channel_map=aux0,aux1".to_owned())
    );
    assert!(
        plan.modules[1]
            .args
            .contains(&"master_channel_map=aux0".to_owned())
    );
    assert!(plan.modules[2].args.contains(&"sink=hdmi-sink".to_owned()));
    assert!(
        plan.modules[3]
            .args
            .contains(&"master_channel_map=aux1".to_owned())
    );
    assert!(plan.modules[4].args.contains(&"sink=q900t-sink".to_owned()));
}

#[test]
fn calibration_plan_accepts_sixteen_outputs_and_rejects_a_seventeenth() {
    use q900_bridge::calibration::calibration_plan;

    let outputs: Vec<_> = (0..16).map(|index| format!("sink-{index}")).collect();
    let plan = calibration_plan("brio", &outputs, 9).unwrap();
    assert!(plan.modules[0].args.contains(&"channels=16".to_owned()));
    assert!(plan.modules[0].args.contains(
        &"channel_map=aux0,aux1,aux2,aux3,aux4,aux5,aux6,aux7,aux8,aux9,aux10,aux11,aux12,aux13,aux14,aux15".to_owned()
    ));
    assert!(calibration_plan("brio", &outputs[..1], 9).is_err());

    let mut too_many = outputs;
    too_many.push("sink-16".to_owned());
    assert!(calibration_plan("brio", &too_many, 9).is_err());
}

#[test]
fn estimates_probe_arrival_under_broadband_capture_noise() {
    use q900_bridge::calibration::generate_probe;

    let probe = generate_probe(48_000, 7).unwrap();
    let expected_offset = 9_600;
    let mut state = 0x1234_5678_u32;
    let mut recording = Vec::with_capacity(48_000 * 2);
    for _ in 0..48_000 * 2 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let noise = (state as i32 as f32 / i32::MAX as f32) * 0.03;
        recording.push(noise);
    }
    for (index, sample) in probe.iter().enumerate() {
        recording[expected_offset + index] += sample;
    }

    let estimate = estimate_arrival(&probe, &recording, 1, 48_000).unwrap();

    assert!(estimate.sample_offset.abs_diff(expected_offset) <= 3);
    assert!(estimate.confidence > 0.80);
}

#[test]
fn estimates_a_distinct_probe_with_the_weak_correlation_seen_on_hdmi() {
    use q900_bridge::calibration::generate_probe;

    let probe = generate_probe(48_000, 31).unwrap();
    let expected_offset = 50_544;
    let mut recording = vec![0.0; (expected_offset + probe.len()) * 2];
    let mut left_state = 0x5a17_2f31_u32;
    let mut right_state = 0x792b_4c13_u32;
    for frame in 0..recording.len() / 2 {
        left_state ^= left_state << 13;
        left_state ^= left_state >> 17;
        left_state ^= left_state << 5;
        right_state ^= right_state << 13;
        right_state ^= right_state >> 17;
        right_state ^= right_state << 5;
        let left_noise = ((left_state >> 8) as f32 / 8_388_607.5 - 1.0) * 0.00097;
        let right_noise = ((right_state >> 8) as f32 / 8_388_607.5 - 1.0) * 0.00097;
        recording[frame * 2] = left_noise;
        recording[frame * 2 + 1] = right_noise;
    }
    for (index, sample) in probe.iter().enumerate() {
        recording[(expected_offset + index) * 2 + 1] += sample * 0.001;
    }

    let estimate = estimate_arrival(&probe, &recording, 2, 48_000).unwrap();

    assert_eq!(estimate.sample_offset, expected_offset);
    assert_eq!(estimate.channel_index, 1);
    assert!((0.15..0.18).contains(&estimate.confidence));
    assert!(estimate.peak_to_sidelobe_ratio > 1.15);
}

#[test]
fn random_capture_noise_without_the_probe_is_rejected() {
    let mut state = 0x2345_6789_u32;
    let recording: Vec<_> = (0..48_000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            ((state >> 8) as f32 / 8_388_607.5 - 1.0) * 0.00105
        })
        .collect();
    let probe = q900_bridge::calibration::generate_probe(48_000, 31).unwrap();

    let error = estimate_arrival(&probe, &recording, 1, 48_000).unwrap_err();

    assert!(error.contains("too weak") || error.contains("ambiguous"));
    assert!(error.contains("peak-to-sidelobe ratio"));
}

#[test]
fn rejects_silence_and_clipped_probe_recordings() {
    use q900_bridge::calibration::generate_probe;

    let probe = generate_probe(48_000, 11).unwrap();
    let silent = vec![0.0; 48_000];
    let silence_error = estimate_arrival(&probe, &silent, 1, 48_000).unwrap_err();
    assert!(silence_error.contains("too weak"));

    let offset = 4_800;
    let mut clipped = vec![0.0; 48_000];
    for (index, sample) in probe.iter().enumerate() {
        clipped[offset + index] = (sample * 20.0).clamp(-1.0, 1.0);
    }
    let clipping_error = estimate_arrival(&probe, &clipped, 1, 48_000).unwrap_err();
    assert!(clipping_error.contains("clipped"));
}

#[test]
fn rejects_two_equally_strong_probe_arrivals_as_ambiguous() {
    use q900_bridge::calibration::generate_probe;

    let probe = generate_probe(48_000, 23).unwrap();
    let first = 4_800;
    let second = 19_200;
    let mut recording = vec![0.0; 48_000];
    for offset in [first, second] {
        for (index, sample) in probe.iter().enumerate() {
            recording[offset + index] += sample;
        }
    }

    let error = estimate_arrival(&probe, &recording, 1, 48_000).unwrap_err();

    assert!(error.contains("ambiguous"));
}

#[test]
fn calibration_report_keeps_repeats_and_uses_the_slowest_path_as_reference() {
    use q900_bridge::calibration::build_report;

    let outputs = vec!["hdmi".to_owned(), "q900t".to_owned()];
    let readings = vec![
        vec![100.0, 100.5, 99.5]
            .into_iter()
            .map(arrival_estimate)
            .collect(),
        vec![220.0, 221.0, 219.0]
            .into_iter()
            .map(arrival_estimate)
            .collect(),
    ];

    let report = build_report("brio", &outputs, &readings, 48_000).unwrap();

    assert_eq!(report.repetitions, 3);
    assert_eq!(report.outputs[0].median_arrival_ms, 100.0);
    assert_eq!(report.outputs[0].delay_ms, 120);
    assert_eq!(report.outputs[1].median_arrival_ms, 220.0);
    assert_eq!(report.outputs[1].delay_ms, 0);
    assert_eq!(report.median_relative_skew_ms, 120.0);
    assert_eq!(report.maximum_relative_skew_ms, 120.5);
}

#[test]
fn build_report_rejects_a_measurement_below_either_quality_threshold() {
    use q900_bridge::calibration::build_report;

    let outputs = vec!["hdmi".to_owned(), "q900t".to_owned()];
    let mut readings = vec![
        vec![arrival_estimate(100.0); 3],
        vec![arrival_estimate(200.0); 3],
    ];
    readings[0][0].confidence = 0.139;
    assert!(build_report("brio", &outputs, &readings, 48_000).is_err());

    readings[0][0] = arrival_estimate(100.0);
    readings[0][0].peak_to_sidelobe_ratio = 1.149;
    assert!(build_report("brio", &outputs, &readings, 48_000).is_err());
}

#[test]
fn build_report_rejects_non_finite_or_negative_measurements() {
    use q900_bridge::calibration::build_report;

    let outputs = vec!["hdmi".to_owned(), "q900t".to_owned()];
    let invalid_readings = [
        ArrivalEstimate {
            arrival_ms: f64::NAN,
            ..arrival_estimate(100.0)
        },
        ArrivalEstimate {
            arrival_ms: -1.0,
            ..arrival_estimate(100.0)
        },
        ArrivalEstimate {
            confidence: f64::NAN,
            ..arrival_estimate(100.0)
        },
        ArrivalEstimate {
            confidence: 1.01,
            ..arrival_estimate(100.0)
        },
        ArrivalEstimate {
            peak_to_sidelobe_ratio: f64::NAN,
            ..arrival_estimate(100.0)
        },
        ArrivalEstimate {
            peak_to_sidelobe_ratio: -0.1,
            ..arrival_estimate(100.0)
        },
    ];

    for invalid in invalid_readings {
        let readings = vec![
            vec![invalid, arrival_estimate(100.0), arrival_estimate(100.0)],
            vec![arrival_estimate(200.0); 3],
        ];
        assert!(build_report("brio", &outputs, &readings, 48_000).is_err());
    }
}

fn arrival_estimate(arrival_ms: f64) -> q900_bridge::calibration::ArrivalEstimate {
    q900_bridge::calibration::ArrivalEstimate {
        sample_offset: (arrival_ms * 48.0).round() as usize,
        arrival_ms,
        channel_index: 0,
        confidence: 0.95,
        peak_to_sidelobe_ratio: 4.0,
    }
}
