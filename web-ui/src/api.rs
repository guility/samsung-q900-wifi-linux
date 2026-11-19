use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WebSettings {
    pub selected_microphone: Option<String>,
    pub outputs: Vec<SelectedOutput>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SelectedOutput {
    pub sink: String,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MicrophoneDevice {
    pub name: String,
    pub description: String,
    pub sample_spec: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlaybackDevice {
    pub name: String,
    pub description: String,
    pub sample_spec: String,
    pub state: String,
    pub channels: usize,
    pub positions: Vec<String>,
    pub transport: String,
    pub stereo_supported: bool,
    pub stereo_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UiState {
    pub microphones: Vec<MicrophoneDevice>,
    pub outputs: Vec<PlaybackDevice>,
    pub default_microphone: Option<String>,
    pub selected_microphone: Option<String>,
    pub selected_outputs: Vec<SelectedOutput>,
    pub mirror_running: bool,
    pub calibration_available: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MirrorPreview {
    pub output: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalibrationMeasurement {
    pub arrival_ms: f64,
    pub confidence: f64,
    pub peak_to_sidelobe_ratio: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalibrationOutput {
    pub sink: String,
    pub measurements: Vec<CalibrationMeasurement>,
    pub median_arrival_ms: f64,
    pub minimum_arrival_ms: f64,
    pub maximum_arrival_ms: f64,
    pub confidence: f64,
    pub peak_to_sidelobe_ratio: f64,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CalibrationResult {
    pub microphone: String,
    pub sample_rate: u32,
    pub repetitions: usize,
    pub median_relative_skew_ms: f64,
    pub maximum_relative_skew_ms: f64,
    pub outputs: Vec<CalibrationOutput>,
}

impl CalibrationResult {
    pub fn matches_settings(&self, settings: &WebSettings) -> bool {
        settings.selected_microphone.as_deref() == Some(self.microphone.as_str())
            && self.outputs.len() == settings.outputs.len()
            && self.outputs.iter().all(|measured| {
                settings
                    .outputs
                    .iter()
                    .any(|selected| selected.sink == measured.sink)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> CalibrationResult {
        CalibrationResult {
            microphone: "brio".to_owned(),
            sample_rate: 48_000,
            repetitions: 3,
            median_relative_skew_ms: 10.0,
            maximum_relative_skew_ms: 12.0,
            outputs: vec![
                CalibrationOutput {
                    sink: "hdmi".to_owned(),
                    measurements: Vec::new(),
                    median_arrival_ms: 20.0,
                    minimum_arrival_ms: 19.0,
                    maximum_arrival_ms: 21.0,
                    confidence: 0.9,
                    peak_to_sidelobe_ratio: 2.0,
                    delay_ms: 10,
                },
                CalibrationOutput {
                    sink: "bluetooth".to_owned(),
                    measurements: Vec::new(),
                    median_arrival_ms: 30.0,
                    minimum_arrival_ms: 29.0,
                    maximum_arrival_ms: 31.0,
                    confidence: 0.8,
                    peak_to_sidelobe_ratio: 1.8,
                    delay_ms: 0,
                },
            ],
        }
    }

    #[test]
    fn calibration_report_matches_the_microphone_and_selected_outputs() {
        let report = report();
        let settings = WebSettings {
            selected_microphone: Some("brio".to_owned()),
            outputs: vec![
                SelectedOutput {
                    sink: "bluetooth".to_owned(),
                    delay_ms: 15,
                },
                SelectedOutput {
                    sink: "hdmi".to_owned(),
                    delay_ms: 0,
                },
            ],
        };

        assert!(report.matches_settings(&settings));
    }

    #[test]
    fn calibration_report_does_not_match_after_microphone_changes_or_is_cleared() {
        let report = report();
        let settings = WebSettings {
            selected_microphone: Some("other-mic".to_owned()),
            outputs: vec![
                SelectedOutput {
                    sink: "hdmi".to_owned(),
                    delay_ms: 0,
                },
                SelectedOutput {
                    sink: "bluetooth".to_owned(),
                    delay_ms: 0,
                },
            ],
        };
        assert!(!report.matches_settings(&settings));

        let mut settings_without_microphone = settings;
        settings_without_microphone.selected_microphone = None;
        assert!(!report.matches_settings(&settings_without_microphone));
    }

    #[test]
    fn calibration_report_does_not_match_when_outputs_change() {
        let report = report();
        let settings = WebSettings {
            selected_microphone: Some("brio".to_owned()),
            outputs: vec![SelectedOutput {
                sink: "hdmi".to_owned(),
                delay_ms: 0,
            }],
        };
        assert!(!report.matches_settings(&settings));
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub error: String,
}
