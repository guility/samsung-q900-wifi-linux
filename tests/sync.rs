use q900_bridge::sync::{OutputPath, SyncObservation, analyze_sync, parse_sync_csv};

#[test]
fn reports_fixed_skew_drift_and_compensation_from_acoustic_observations() {
    let observations = [
        SyncObservation::new(10.0, 250.0, 262.0),
        SyncObservation::new(40.0, 260.0, 278.0),
        SyncObservation::new(70.0, 300.0, 324.0),
        SyncObservation::new(100.0, 340.0, 370.0),
    ];

    let report = analyze_sync(&observations).unwrap();

    assert_eq!(report.observations, 4);
    assert!((report.q900_offset_at_start_ms - 12.0).abs() < 0.001);
    assert!((report.drift_ms_per_minute - 12.0).abs() < 0.001);
    assert!((report.median_absolute_skew_ms - 21.0).abs() < 0.001);
    assert!((report.p95_absolute_skew_ms - 30.0).abs() < 0.001);
    assert!((report.max_absolute_skew_ms - 30.0).abs() < 0.001);
    assert!((report.p95_jitter_ms).abs() < 0.001);
    assert_eq!(report.delay_earlier_output, Some(OutputPath::Local));
    assert!((report.recommended_delay_ms - 12.0).abs() < 0.001);
}

#[test]
fn recommends_delaying_q900_when_local_output_arrives_late() {
    let observations = [
        SyncObservation::new(0.0, 104.0, 100.0),
        SyncObservation::new(60.0, 204.0, 200.0),
        SyncObservation::new(120.0, 304.0, 300.0),
    ];

    let report = analyze_sync(&observations).unwrap();

    assert!((report.q900_offset_at_start_ms + 4.0).abs() < 0.001);
    assert_eq!(report.delay_earlier_output, Some(OutputPath::Q900T));
    assert!((report.recommended_delay_ms - 4.0).abs() < 0.001);
    assert!(report.max_absolute_skew_ms <= 4.0);
}

#[test]
fn rejects_too_few_or_out_of_order_acoustic_observations() {
    let one = [SyncObservation::new(0.0, 0.0, 0.0)];
    assert!(analyze_sync(&one).unwrap_err().contains("at least three"));

    let two = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1.0, 0.0, 0.0),
    ];
    assert!(analyze_sync(&two).unwrap_err().contains("at least three"));

    let out_of_order = [
        SyncObservation::new(3.0, 0.0, 0.0),
        SyncObservation::new(2.0, 0.0, 0.0),
        SyncObservation::new(4.0, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&out_of_order)
            .unwrap_err()
            .contains("observation 2")
    );

    let duplicate_time = [
        SyncObservation::new(3.0, 0.0, 0.0),
        SyncObservation::new(4.0, 0.0, 0.0),
        SyncObservation::new(4.0, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&duplicate_time)
            .unwrap_err()
            .contains("observation 3")
    );

    let negative_time = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(-1.0, 0.0, 0.0),
        SyncObservation::new(2.0, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&negative_time)
            .unwrap_err()
            .contains("observation 2")
    );

    let non_finite = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1.0, 0.0, f64::NAN),
        SyncObservation::new(2.0, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&non_finite)
            .unwrap_err()
            .contains("non-finite")
    );
    assert!(
        analyze_sync(&non_finite)
            .unwrap_err()
            .contains("observation 2")
    );

    let non_finite_local_arrival = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1.0, f64::NAN, 0.0),
        SyncObservation::new(2.0, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&non_finite_local_arrival)
            .unwrap_err()
            .contains("non-finite")
    );

    let overflowed_skew = [
        SyncObservation::new(0.0, -f64::MAX, f64::MAX),
        SyncObservation::new(1.0, -f64::MAX, f64::MAX),
        SyncObservation::new(2.0, -f64::MAX, f64::MAX),
    ];
    assert!(
        analyze_sync(&overflowed_skew)
            .unwrap_err()
            .contains("skew is outside the supported numeric range")
    );

    let overflowing_time_variance = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1e200, 1e203, 1e203),
        SyncObservation::new(2e200, 2e203, 2e203),
    ];
    assert!(
        analyze_sync(&overflowing_time_variance)
            .unwrap_err()
            .contains("time variance is outside the supported numeric range")
    );
}

#[test]
fn exact_alignment_has_no_delay_recommendation() {
    let observations = [
        SyncObservation::new(0.0, 100.0, 100.0),
        SyncObservation::new(60.0, 200.0, 200.0),
        SyncObservation::new(120.0, 300.0, 300.0),
    ];

    let report = analyze_sync(&observations).unwrap();

    assert_eq!(report.delay_earlier_output, None);
    assert_eq!(report.recommended_delay_ms, 0.0);
    assert_eq!(report.q900_offset_at_start_ms, 0.0);
}

#[test]
fn p95_nearest_rank_excludes_a_single_late_outlier_from_twenty_one_samples() {
    let observations: Vec<_> = (0..21)
        .map(|index| {
            let elapsed_seconds = index as f64 * 10.0;
            let local_arrival_ms = elapsed_seconds * 1000.0;
            let skew_ms = if index == 20 { 100.0 } else { 1.0 };
            SyncObservation::new(
                elapsed_seconds,
                local_arrival_ms,
                local_arrival_ms + skew_ms,
            )
        })
        .collect();

    let report = analyze_sync(&observations).unwrap();

    assert_eq!(report.p95_absolute_skew_ms, 1.0);
    assert_eq!(report.max_absolute_skew_ms, 100.0);
}

#[test]
fn csv_errors_report_one_based_source_line_numbers() {
    let wrong_column_count = "elapsed_s,local_ms,q900_ms\n0,100\n";
    assert!(
        parse_sync_csv(wrong_column_count)
            .unwrap_err()
            .contains("CSV line 2")
    );

    let invalid_number = "elapsed_s,local_ms,q900_ms\n0,100,101\n60,late,162\n";
    assert!(
        parse_sync_csv(invalid_number)
            .unwrap_err()
            .contains("CSV line 3")
    );
}

#[test]
fn csv_parses_header_and_arrival_columns_in_the_declared_order() {
    let csv = concat!(
        "# single recorder clock\n\n",
        "elapsed_s,local_ms,q900_ms\n",
        "0,100,108\n",
        "60,60100,60108\n",
        "120,120100,120108\n"
    );

    let observations = parse_sync_csv(csv).unwrap();

    assert_eq!(observations.len(), 3);
    assert_eq!(observations[0].elapsed_seconds, 0.0);
    assert_eq!(observations[0].local_arrival_ms, 100.0);
    assert_eq!(observations[0].q900_arrival_ms, 108.0);
}

#[test]
fn reports_which_regression_intermediate_exceeded_numeric_range() {
    let overflowed_mean_skew = [
        SyncObservation::new(0.0, 0.0, 1e308),
        SyncObservation::new(1.0, 0.0, 1e308),
        SyncObservation::new(2.0, 0.0, 1e308),
    ];
    assert!(
        analyze_sync(&overflowed_mean_skew)
            .unwrap_err()
            .contains("mean skew is outside the supported numeric range")
    );

    let overflowed_covariance = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1e153, 0.0, 1e155),
        SyncObservation::new(2e153, 0.0, 2e155),
    ];
    assert!(
        analyze_sync(&overflowed_covariance)
            .unwrap_err()
            .contains("covariance is outside the supported numeric range")
    );

    let overflowed_variance = [
        SyncObservation::new(0.0, 0.0, 0.0),
        SyncObservation::new(1e200, 0.0, 0.0),
        SyncObservation::new(2e200, 0.0, 0.0),
    ];
    assert!(
        analyze_sync(&overflowed_variance)
            .unwrap_err()
            .contains("time variance is outside the supported numeric range")
    );

    let overflowed_drift = [
        SyncObservation::new(0.0, 0.0, -1e308),
        SyncObservation::new(1e-150, 0.0, 0.0),
        SyncObservation::new(2e-150, 0.0, 1e308),
    ];
    assert!(
        analyze_sync(&overflowed_drift)
            .unwrap_err()
            .contains("fitted drift is outside the supported numeric range")
    );
}
