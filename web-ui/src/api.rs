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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub error: String,
}
