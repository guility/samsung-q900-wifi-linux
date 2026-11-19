const MIN_PROBE_CORRELATION: f64 = 0.20;
const MIN_PEAK_TO_SIDELOBE_RATIO: f64 = 1.15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrivalEstimate {
    /// Sample index of the probe's first arrival in the recorded stream.
    pub sample_offset: usize,
    pub arrival_ms: f64,
    pub channel_index: usize,
    /// Absolute normalized correlation in the range 0..=1.
    pub confidence: f64,
    /// Best peak divided by the strongest non-adjacent competing peak.
    pub peak_to_sidelobe_ratio: f64,
}

/// Finds a known probe in interleaved floating-point PCM captured from one source.
///
/// The capture is downsampled to at most roughly 16 kHz for the matched filter;
/// returned offsets are converted back to the source sample rate.
pub fn estimate_arrival(
    probe: &[f32],
    recording: &[f32],
    channels: usize,
    sample_rate: u32,
) -> Result<ArrivalEstimate, String> {
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err("sample rate must be between 8000 and 192000 Hz".to_owned());
    }
    if !(1..=32).contains(&channels) {
        return Err("capture must contain between 1 and 32 channels".to_owned());
    }
    if !recording.len().is_multiple_of(channels) {
        return Err("recording length does not contain whole PCM frames".to_owned());
    }
    if probe.len() < (sample_rate as usize / 100) {
        return Err("probe must be at least 10 ms long".to_owned());
    }
    if probe
        .iter()
        .chain(recording)
        .any(|sample| !sample.is_finite())
    {
        return Err("probe and recording must contain only finite samples".to_owned());
    }

    let stride = (sample_rate as usize / 16_000).max(1);
    let probe: Vec<f64> = probe
        .iter()
        .step_by(stride)
        .map(|sample| *sample as f64)
        .collect();
    if probe.len() < 32 {
        return Err("probe is too short for correlation".to_owned());
    }
    let probe_mean = mean(&probe);
    let centered_probe: Vec<f64> = probe.iter().map(|sample| sample - probe_mean).collect();
    let probe_energy = centered_probe
        .iter()
        .map(|sample| sample * sample)
        .sum::<f64>();
    if probe_energy <= 1e-12 {
        return Err("probe is silent".to_owned());
    }

    let frame_count = recording.len() / channels;
    if frame_count < (probe.len() - 1) * stride + 1 {
        return Err("recording is shorter than the probe".to_owned());
    }
    let separation = (sample_rate as usize / 100 / stride).max(1);
    let mut candidates = Vec::with_capacity(channels);

    for channel_index in 0..channels {
        let samples: Vec<f64> = (0..frame_count)
            .step_by(stride)
            .map(|frame| recording[frame * channels + channel_index] as f64)
            .collect();
        let (best_offset, confidence, sidelobe) =
            correlate(&centered_probe, probe_energy, &samples, separation);
        candidates.push((best_offset, confidence, sidelobe, channel_index));
    }

    let Some((offset, confidence, sidelobe, channel_index)) = candidates
        .into_iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))
    else {
        return Err("capture has no usable channels".to_owned());
    };
    if confidence < MIN_PROBE_CORRELATION {
        return Err(format!(
            "probe response is too weak (correlation {confidence:.3}; minimum {MIN_PROBE_CORRELATION:.3})"
        ));
    }
    if confidence / sidelobe.max(1e-9) < MIN_PEAK_TO_SIDELOBE_RATIO {
        return Err(format!(
            "probe response is ambiguous (peak-to-sidelobe ratio {:.2}; minimum {:.2})",
            confidence / sidelobe.max(1e-9),
            MIN_PEAK_TO_SIDELOBE_RATIO
        ));
    }

    let sample_offset = (offset * stride).min(frame_count.saturating_sub(1));
    let end = sample_offset
        .saturating_add(probe.len().saturating_mul(stride))
        .min(frame_count);
    let clipped_samples = (sample_offset..end)
        .filter(|frame| recording[frame * channels + channel_index].abs() >= 0.999)
        .count();
    let window_length = end.saturating_sub(sample_offset).max(1);
    if clipped_samples as f64 / window_length as f64 > 0.005 {
        return Err("probe response is clipped; lower the playback or microphone level".to_owned());
    }

    let sample_offset = offset * stride;
    Ok(ArrivalEstimate {
        sample_offset,
        arrival_ms: sample_offset as f64 * 1000.0 / sample_rate as f64,
        channel_index,
        confidence,
        peak_to_sidelobe_ratio: confidence / sidelobe.max(1e-9),
    })
}

fn correlate(
    probe: &[f64],
    probe_energy: f64,
    recording: &[f64],
    separation: usize,
) -> (usize, f64, f64) {
    let window_count = recording.len() - probe.len() + 1;
    let mut prefix_sum = Vec::with_capacity(recording.len() + 1);
    let mut prefix_square_sum = Vec::with_capacity(recording.len() + 1);
    prefix_sum.push(0.0);
    prefix_square_sum.push(0.0);
    for sample in recording {
        prefix_sum.push(prefix_sum.last().copied().unwrap_or_default() + sample);
        prefix_square_sum
            .push(prefix_square_sum.last().copied().unwrap_or_default() + sample * sample);
    }

    let mut scores = Vec::with_capacity(window_count);
    for offset in 0..window_count {
        let sum = prefix_sum[offset + probe.len()] - prefix_sum[offset];
        let square_sum = prefix_square_sum[offset + probe.len()] - prefix_square_sum[offset];
        let centered_energy = (square_sum - sum * sum / probe.len() as f64).max(0.0);
        if centered_energy <= 1e-12 {
            scores.push(0.0);
            continue;
        }
        let dot = probe
            .iter()
            .enumerate()
            .map(|(index, sample)| sample * recording[offset + index])
            .sum::<f64>();
        scores.push((dot / (probe_energy * centered_energy).sqrt()).abs());
    }

    let best_offset = scores
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
        .unwrap_or_default();
    let confidence = scores.get(best_offset).copied().unwrap_or_default();
    let sidelobe = scores
        .iter()
        .enumerate()
        .filter(|(index, _)| index.abs_diff(best_offset) > separation)
        .map(|(_, score)| *score)
        .fold(0.0, f64::max);
    (best_offset, confidence, sidelobe)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathArrival {
    pub sink: String,
    /// Repeated arrival estimates from the same microphone clock, in milliseconds.
    pub arrivals_ms: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DelayProposal {
    pub sink: String,
    pub median_arrival_ms: f64,
    /// Integer milliseconds to add so every earlier path is delayed to the slowest path.
    pub delay_ms: u16,
}

/// Proposes non-negative fixed delays from repeated acoustic arrival measurements.
pub fn propose_delays(arrivals: &[PathArrival]) -> Result<Vec<DelayProposal>, String> {
    if arrivals.len() < 2 {
        return Err("at least two output paths are required".to_owned());
    }
    let mut seen = std::collections::HashSet::new();
    let mut medians = Vec::with_capacity(arrivals.len());
    for path in arrivals {
        if path.sink.trim().is_empty() {
            return Err("output path names must not be empty".to_owned());
        }
        if !seen.insert(path.sink.as_str()) {
            return Err(format!("output path {:?} is repeated", path.sink));
        }
        if path.arrivals_ms.len() < 3 {
            return Err(format!(
                "output path {:?} needs at least three measurements",
                path.sink
            ));
        }
        if path
            .arrivals_ms
            .iter()
            .any(|arrival| !arrival.is_finite() || *arrival < 0.0)
        {
            return Err(format!(
                "output path {:?} has an invalid arrival time",
                path.sink
            ));
        }
        medians.push(median(&path.arrivals_ms));
    }

    let slowest = medians.iter().copied().fold(0.0, f64::max);
    arrivals
        .iter()
        .zip(medians)
        .map(|(path, median_arrival_ms)| {
            let delay_ms = (slowest - median_arrival_ms).max(0.0).ceil();
            if delay_ms > 2_000.0 {
                return Err(format!("compensation for {:?} exceeds 2000 ms", path.sink));
            }
            Ok(DelayProposal {
                sink: path.sink.clone(),
                median_arrival_ms,
                delay_ms: delay_ms as u16,
            })
        })
        .collect()
}

fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

/// Builds a low-level, broadband probe with a deterministic code for one output.
pub fn generate_probe(sample_rate: u32, sequence_id: usize) -> Result<Vec<f32>, String> {
    if !(16_000..=96_000).contains(&sample_rate) {
        return Err("probe sample rate must be between 16000 and 96000 Hz".to_owned());
    }
    let chip_samples = (sample_rate as usize / 5_000).max(1);
    let fade_samples = (sample_rate as usize / 200).max(1);
    let mut state = (sequence_id as u32)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(0x85eb_ca6b);
    if state == 0 {
        state = 0x6d2b_79f5;
    }
    let mut probe = Vec::with_capacity(1023 * chip_samples);
    for _ in 0..1023 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let chip = if state & 1 == 0 { -0.1 } else { 0.1 };
        probe.extend(std::iter::repeat_n(chip, chip_samples));
    }
    let last = probe.len().saturating_sub(1);
    for (index, sample) in probe.iter_mut().enumerate() {
        let fade_in = (index as f64 / fade_samples as f64).min(1.0);
        let fade_out = ((last.saturating_sub(index)) as f64 / fade_samples as f64).min(1.0);
        let envelope = (fade_in.min(fade_out) * std::f64::consts::FRAC_PI_2)
            .sin()
            .powi(2);
        *sample *= envelope as f32;
    }
    Ok(probe)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalibrationPlan {
    pub microphone: String,
    pub outputs: Vec<String>,
    pub bus_sink: String,
    pub sample_rate: u32,
    pub repetitions: usize,
    pub modules: Vec<crate::ModuleSpec>,
}

/// Builds a PipeWire Pulse module plan for a synchronized multichannel probe.
pub fn calibration_plan(
    microphone: &str,
    outputs: &[String],
    session_id: u32,
) -> Result<CalibrationPlan, String> {
    if microphone.trim().is_empty() {
        return Err("calibration requires a microphone source".to_owned());
    }
    if outputs.len() < 2 {
        return Err("calibration requires at least two playback outputs".to_owned());
    }
    if outputs.len() > 16 {
        return Err("calibration supports at most 16 playback outputs".to_owned());
    }
    let mut seen = std::collections::HashSet::new();
    for output in outputs {
        if output.trim().is_empty() {
            return Err("output sink names must not be empty".to_owned());
        }
        if !seen.insert(output.as_str()) {
            return Err(format!("output sink {output:?} is selected more than once"));
        }
    }

    let bus_sink = format!("q900_calibration_{session_id}");
    let channel_map = (0..outputs.len())
        .map(|index| format!("aux{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut modules = vec![crate::ModuleSpec {
        name: "module-null-sink".to_owned(),
        args: vec![
            format!("sink_name={bus_sink}"),
            "format=s16le".to_owned(),
            "rate=48000".to_owned(),
            format!("channels={}", outputs.len()),
            format!("channel_map={channel_map}"),
            "sink_properties=device.description=Q900_Calibration_Bus".to_owned(),
        ],
    }];

    for (index, sink) in outputs.iter().enumerate() {
        let source = format!("q900_calibration_source_{session_id}_{index}");
        modules.push(crate::ModuleSpec {
            name: "module-remap-source".to_owned(),
            args: vec![
                format!("source_name={source}"),
                format!("master={bus_sink}.monitor"),
                format!("master_channel_map=aux{index}"),
                "format=s16le".to_owned(),
                "rate=48000".to_owned(),
                "channels=1".to_owned(),
                "channel_map=mono".to_owned(),
                "remix=0".to_owned(),
                format!("source_properties=device.description=Q900_Calibration_Source_{index}"),
            ],
        });
        modules.push(crate::ModuleSpec {
            name: "module-loopback".to_owned(),
            args: vec![
                format!("source={source}"),
                format!("sink={sink}"),
                "rate=48000".to_owned(),
                "channels=2".to_owned(),
                "channel_map=front-left,front-right".to_owned(),
                "latency_msec=60".to_owned(),
                "source_dont_move=true".to_owned(),
                "sink_dont_move=true".to_owned(),
                "remix=true".to_owned(),
            ],
        });
    }

    Ok(CalibrationPlan {
        microphone: microphone.to_owned(),
        outputs: outputs.to_vec(),
        bus_sink,
        sample_rate: 48_000,
        repetitions: 3,
        modules,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputCalibration {
    pub sink: String,
    pub measurements: Vec<ArrivalEstimate>,
    pub median_arrival_ms: f64,
    pub minimum_arrival_ms: f64,
    pub maximum_arrival_ms: f64,
    pub confidence: f64,
    pub peak_to_sidelobe_ratio: f64,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalibrationReport {
    pub microphone: String,
    pub sample_rate: u32,
    pub repetitions: usize,
    pub median_relative_skew_ms: f64,
    pub maximum_relative_skew_ms: f64,
    pub outputs: Vec<OutputCalibration>,
}

/// Summarizes repeated arrivals from one recorder clock and proposes path delays.
pub fn build_report(
    microphone: &str,
    outputs: &[String],
    measurements: &[Vec<ArrivalEstimate>],
    sample_rate: u32,
) -> Result<CalibrationReport, String> {
    if microphone.trim().is_empty() {
        return Err("calibration report requires a microphone source".to_owned());
    }
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err("sample rate must be between 8000 and 192000 Hz".to_owned());
    }
    if outputs.len() < 2 || outputs.len() != measurements.len() {
        return Err(
            "calibration report needs one measurement series for each of at least two outputs"
                .to_owned(),
        );
    }
    let repetitions = measurements.first().map(Vec::len).unwrap_or_default();
    if repetitions < 3
        || measurements
            .iter()
            .any(|series| series.len() != repetitions)
    {
        return Err("each output needs the same number of at least three measurements".to_owned());
    }

    let mut paths = Vec::with_capacity(outputs.len());
    for (sink, series) in outputs.iter().zip(measurements) {
        let arrivals_ms: Vec<_> = series.iter().map(|reading| reading.arrival_ms).collect();
        if series.iter().any(|reading| {
            !reading.arrival_ms.is_finite()
                || reading.arrival_ms < 0.0
                || !reading.confidence.is_finite()
                || !(0.0..=1.0).contains(&reading.confidence)
                || reading.confidence < MIN_PROBE_CORRELATION
                || !reading.peak_to_sidelobe_ratio.is_finite()
                || reading.peak_to_sidelobe_ratio < 0.0
                || reading.peak_to_sidelobe_ratio < MIN_PEAK_TO_SIDELOBE_RATIO
        }) {
            return Err(format!(
                "output path {sink:?} contains an invalid measurement"
            ));
        }
        paths.push(PathArrival {
            sink: sink.clone(),
            arrivals_ms,
        });
    }
    let proposals = propose_delays(&paths)?;
    let mut relative_skews = Vec::with_capacity(repetitions);
    for repetition in 0..repetitions {
        let earliest = measurements
            .iter()
            .map(|series| series[repetition].arrival_ms)
            .fold(f64::INFINITY, f64::min);
        let latest = measurements
            .iter()
            .map(|series| series[repetition].arrival_ms)
            .fold(f64::NEG_INFINITY, f64::max);
        relative_skews.push(latest - earliest);
    }
    let maximum_relative_skew_ms = relative_skews.iter().copied().fold(0.0, f64::max);
    let median_relative_skew_ms = median(&relative_skews);

    let outputs = outputs
        .iter()
        .zip(measurements)
        .zip(proposals)
        .map(|((sink, series), proposal)| {
            let arrivals_ms: Vec<_> = series.iter().map(|reading| reading.arrival_ms).collect();
            OutputCalibration {
                sink: sink.clone(),
                measurements: series.clone(),
                median_arrival_ms: proposal.median_arrival_ms,
                minimum_arrival_ms: arrivals_ms.iter().copied().fold(f64::INFINITY, f64::min),
                maximum_arrival_ms: arrivals_ms
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max),
                confidence: series
                    .iter()
                    .map(|reading| reading.confidence)
                    .fold(f64::INFINITY, f64::min),
                peak_to_sidelobe_ratio: series
                    .iter()
                    .map(|reading| reading.peak_to_sidelobe_ratio)
                    .fold(f64::INFINITY, f64::min),
                delay_ms: proposal.delay_ms,
            }
        })
        .collect();

    Ok(CalibrationReport {
        microphone: microphone.to_owned(),
        sample_rate,
        repetitions,
        median_relative_skew_ms,
        maximum_relative_skew_ms,
        outputs,
    })
}
