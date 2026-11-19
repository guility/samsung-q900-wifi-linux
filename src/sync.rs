#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncObservation {
    pub elapsed_seconds: f64,
    pub local_arrival_ms: f64,
    pub q900_arrival_ms: f64,
}

impl SyncObservation {
    pub fn new(elapsed_seconds: f64, local_arrival_ms: f64, q900_arrival_ms: f64) -> Self {
        Self {
            elapsed_seconds,
            local_arrival_ms,
            q900_arrival_ms,
        }
    }

    pub fn skew_ms(self) -> f64 {
        self.q900_arrival_ms - self.local_arrival_ms
    }
}

/// Parses rows of `elapsed_s,local_ms,q900_ms` acoustic arrival measurements.
pub fn parse_sync_csv(input: &str) -> Result<Vec<SyncObservation>, String> {
    let mut lines = input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    let (_, header) = lines
        .next()
        .ok_or_else(|| "CSV is empty; expected header elapsed_s,local_ms,q900_ms".to_owned())?;
    let header = header.trim().trim_start_matches('\u{feff}').trim();
    if header != "elapsed_s,local_ms,q900_ms" {
        return Err("expected CSV header elapsed_s,local_ms,q900_ms".to_owned());
    }

    let mut observations = Vec::new();
    for (line_number, line) in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != 3 {
            return Err(format!(
                "CSV line {} must contain elapsed_s,local_ms,q900_ms",
                line_number + 1
            ));
        }
        let parse_field = |index: usize, name: &str| {
            fields[index].parse::<f64>().map_err(|_| {
                format!(
                    "CSV line {} has an invalid {name} value {:?}",
                    line_number + 1,
                    fields[index]
                )
            })
        };
        observations.push(SyncObservation::new(
            parse_field(0, "elapsed_s")?,
            parse_field(1, "local_ms")?,
            parse_field(2, "q900_ms")?,
        ));
    }
    if observations.is_empty() {
        return Err("CSV contains a header but no acoustic observations".to_owned());
    }
    Ok(observations)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputPath {
    Local,
    Q900T,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SyncReport {
    pub observations: usize,
    /// Q900T acoustic arrival minus local acoustic arrival at the first sample.
    /// Positive values mean Q900T is late.
    pub q900_offset_at_start_ms: f64,
    /// Linear skew change in milliseconds per minute.
    pub drift_ms_per_minute: f64,
    pub median_absolute_skew_ms: f64,
    pub p95_absolute_skew_ms: f64,
    pub max_absolute_skew_ms: f64,
    /// Absolute residual after removing the fitted fixed offset and linear drift.
    pub p95_jitter_ms: f64,
    pub max_jitter_ms: f64,
    pub delay_earlier_output: Option<OutputPath>,
    pub recommended_delay_ms: f64,
}

/// Summarizes repeated acoustic arrival measurements for a local output and Q900T.
///
/// The fitted trend separates a fixed acoustic offset from clock drift. The
/// residual statistics describe short-term variation around that trend; they
/// do not prove that either output is synchronized to video or input events.
pub fn analyze_sync(observations: &[SyncObservation]) -> Result<SyncReport, String> {
    if observations.len() < 3 {
        return Err("at least three acoustic observations are required".to_owned());
    }

    for (index, observation) in observations.iter().enumerate() {
        if !observation.elapsed_seconds.is_finite()
            || !observation.local_arrival_ms.is_finite()
            || !observation.q900_arrival_ms.is_finite()
        {
            return Err(format!(
                "observation {} contains a non-finite value",
                index + 1
            ));
        }
        if observation.elapsed_seconds < 0.0 {
            return Err(format!(
                "observation {} has a negative elapsed time",
                index + 1
            ));
        }
        if index > 0 && observation.elapsed_seconds <= observations[index - 1].elapsed_seconds {
            return Err(format!(
                "observation {} must have a later elapsed time than the previous observation",
                index + 1
            ));
        }
    }

    let first_time = observations[0].elapsed_seconds;
    let times: Vec<f64> = observations
        .iter()
        .map(|observation| observation.elapsed_seconds - first_time)
        .collect();
    let skews: Vec<f64> = observations
        .iter()
        .map(|observation| observation.skew_ms())
        .collect();
    if skews.iter().any(|skew| !skew.is_finite()) {
        return Err("skew is outside the supported numeric range".to_owned());
    }
    let mean_time = mean(&times);
    let mean_skew = mean(&skews);
    if !mean_time.is_finite() {
        return Err("mean elapsed time is outside the supported numeric range".to_owned());
    }
    if !mean_skew.is_finite() {
        return Err("mean skew is outside the supported numeric range".to_owned());
    }
    let covariance = times
        .iter()
        .zip(&skews)
        .map(|(time, skew)| (time - mean_time) * (skew - mean_skew))
        .sum::<f64>();
    if !covariance.is_finite() {
        return Err("covariance is outside the supported numeric range".to_owned());
    }
    let time_variance = times
        .iter()
        .map(|time| (time - mean_time).powi(2))
        .sum::<f64>();
    if !time_variance.is_finite() || time_variance <= 0.0 {
        return Err("time variance is outside the supported numeric range".to_owned());
    }

    let drift_ms_per_second = covariance / time_variance;
    if !drift_ms_per_second.is_finite() {
        return Err("fitted drift is outside the supported numeric range".to_owned());
    }
    let offset_at_start_ms = mean_skew - drift_ms_per_second * mean_time;
    if !offset_at_start_ms.is_finite() {
        return Err("fitted offset is outside the supported numeric range".to_owned());
    }
    let absolute_skews: Vec<f64> = skews.iter().map(|skew| skew.abs()).collect();
    let residuals: Vec<f64> = times
        .iter()
        .zip(&skews)
        .map(|(time, skew)| (skew - (offset_at_start_ms + drift_ms_per_second * time)).abs())
        .collect();
    if residuals.iter().any(|residual| !residual.is_finite()) {
        return Err("residual jitter is outside the supported numeric range".to_owned());
    }
    let recommended_delay_ms = offset_at_start_ms.abs();
    let delay_earlier_output = if recommended_delay_ms == 0.0 {
        None
    } else if offset_at_start_ms > 0.0 {
        Some(OutputPath::Local)
    } else {
        Some(OutputPath::Q900T)
    };

    Ok(SyncReport {
        observations: observations.len(),
        q900_offset_at_start_ms: offset_at_start_ms,
        drift_ms_per_minute: drift_ms_per_second * 60.0,
        median_absolute_skew_ms: median(&absolute_skews),
        p95_absolute_skew_ms: percentile_nearest_rank(&absolute_skews, 0.95),
        max_absolute_skew_ms: absolute_skews.iter().copied().fold(0.0, f64::max),
        p95_jitter_ms: percentile_nearest_rank(&residuals, 0.95),
        max_jitter_ms: residuals.iter().copied().fold(0.0, f64::max),
        delay_earlier_output,
        recommended_delay_ms,
    })
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn percentile_nearest_rank(values: &[f64], percentile: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}
