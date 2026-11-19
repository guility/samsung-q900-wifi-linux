use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::uri::{Authority, Uri};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use q900_bridge::{AudioLayout, SinkCapabilities, parse_pipewire_sinks, validate_receiver_sink};
use q900_web_ui::{
    ApiError as ErrorBody, MicrophoneDevice, MirrorPreview, PlaybackDevice, SelectedOutput,
    UiState, WebSettings,
};
use serde_json::Value;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

const WEB_DIST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/web-ui/dist");
const MAX_OUTPUT_DELAY_MS: u16 = 2000;

#[derive(Clone)]
struct WebState {
    inner: Arc<WebStateInner>,
}

struct WebStateInner {
    executable: PathBuf,
    settings: Mutex<WebSettings>,
    mirror: Mutex<Option<Child>>,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl WebState {
    fn new(executable: PathBuf) -> Self {
        let selected_microphone = pactl_output(&["get-default-source"]).ok();
        Self {
            inner: Arc::new(WebStateInner {
                executable,
                settings: Mutex::new(WebSettings {
                    selected_microphone,
                    outputs: Vec::new(),
                }),
                mirror: Mutex::new(None),
            }),
        }
    }

    fn settings(&self) -> Result<WebSettings, ApiError> {
        self.inner
            .settings
            .lock()
            .map(|settings| settings.clone())
            .map_err(|_| ApiError::internal("web settings lock was poisoned"))
    }

    fn set_settings(&self, settings: WebSettings) -> Result<(), ApiError> {
        *self
            .inner
            .settings
            .lock()
            .map_err(|_| ApiError::internal("web settings lock was poisoned"))? = settings;
        Ok(())
    }

    fn mirror_running(&self) -> Result<bool, ApiError> {
        let mut mirror = self
            .inner
            .mirror
            .lock()
            .map_err(|_| ApiError::internal("mirror process lock was poisoned"))?;
        match mirror.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(_)) => {
                    mirror.take();
                    Ok(false)
                }
                Ok(None) => Ok(true),
                Err(error) => Err(ApiError::internal(format!(
                    "could not check mirror process: {error}"
                ))),
            },
            None => Ok(false),
        }
    }

    fn start_mirror(&self, outputs: &[SelectedOutput]) -> Result<(), ApiError> {
        if outputs.len() < 2 {
            return Err(ApiError::bad_request(
                "select at least two stereo playback outputs",
            ));
        }

        let mut mirror = self
            .inner
            .mirror
            .lock()
            .map_err(|_| ApiError::internal("mirror process lock was poisoned"))?;
        if let Some(child) = mirror.as_mut() {
            if child
                .try_wait()
                .map_err(|error| {
                    ApiError::internal(format!("could not check mirror process: {error}"))
                })?
                .is_none()
            {
                return Err(ApiError::conflict("audio mirroring is already running"));
            }
            mirror.take();
        }

        let mut command = Command::new(&self.inner.executable);
        command.arg("mirror");
        for output in outputs {
            command
                .arg("--output")
                .arg(format!("{}={}", output.sink, output.delay_ms));
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| ApiError::internal(format!("could not start mirror: {error}")))?;
        *mirror = Some(child);
        Ok(())
    }

    fn stop_mirror(&self) -> Result<(), ApiError> {
        let child = self
            .inner
            .mirror
            .lock()
            .map_err(|_| ApiError::internal("mirror process lock was poisoned"))?
            .take();
        let Some(mut child) = child else {
            return Ok(());
        };

        if child
            .try_wait()
            .map_err(|error| {
                ApiError::internal(format!("could not check mirror process: {error}"))
            })?
            .is_some()
        {
            return Ok(());
        }

        let pid = child.id().to_string();
        let signal = Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .map_err(|error| ApiError::internal(format!("could not stop mirror: {error}")))?;
        if !signal.success() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ApiError::internal(
                "could not send Ctrl-C to mirror process",
            ));
        }
        let status = child.wait().map_err(|error| {
            ApiError::internal(format!("could not reap mirror process: {error}"))
        })?;
        if !status.success() {
            return Err(ApiError::internal(format!(
                "mirror exited with status {status}"
            )));
        }
        Ok(())
    }
}

pub fn run(port: u16) -> Result<(), String> {
    if !Path::new(WEB_DIST_DIR).join("index.html").exists() {
        return Err(
            "WebAssembly interface is not built; run `cd web-ui && trunk build --release`"
                .to_owned(),
        );
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not find q900-bridge executable: {error}"))?;
    let state = WebState::new(executable);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("could not start web runtime: {error}"))?;
    runtime.block_on(serve(port, state))
}

async fn serve(port: u16, state: WebState) -> Result<(), String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| format!("could not bind web interface to {address}: {error}"))?;
    println!("Audio device selector: http://{address}/");

    let server_state = state.clone();
    let app = router(state);
    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|error| format!("web server failed: {error}"));
    let cleanup_result = server_state
        .stop_mirror()
        .map_err(|error| format!("web shutdown cleanup failed: {}", error.message));
    server_result.and(cleanup_result)
}

fn router(state: WebState) -> Router {
    Router::new()
        .route("/api/state", get(get_state))
        .route("/api/settings", post(update_settings))
        .route("/api/mirror/preview", post(preview_mirror))
        .route("/api/mirror/start", post(start_mirror))
        .route("/api/mirror/stop", post(stop_mirror))
        .fallback_service(ServeDir::new(WEB_DIST_DIR).append_index_html_on_directories(true))
        .layer(middleware::from_fn(local_host_only))
        .with_state(state)
}

async fn get_state(State(state): State<WebState>) -> Result<Json<UiState>, ApiError> {
    ui_state(&state)
}

async fn update_settings(
    State(state): State<WebState>,
    Json(settings): Json<WebSettings>,
) -> Result<Json<UiState>, ApiError> {
    let devices = discover_devices().map_err(ApiError::internal)?;
    validate_settings(&settings, &devices, false)?;
    state.set_settings(settings)?;
    ui_state(&state)
}

async fn start_mirror(State(state): State<WebState>) -> Result<Json<UiState>, ApiError> {
    let devices = discover_devices().map_err(ApiError::internal)?;
    let settings = state.settings()?;
    validate_settings(&settings, &devices, true)?;
    state.start_mirror(&settings.outputs)?;
    ui_state(&state)
}

async fn preview_mirror(State(state): State<WebState>) -> Result<Json<MirrorPreview>, ApiError> {
    let devices = discover_devices().map_err(ApiError::internal)?;
    let settings = state.settings()?;
    validate_settings(&settings, &devices, true)?;
    let mut command = Command::new(&state.inner.executable);
    command.arg("mirror");
    for output in &settings.outputs {
        command
            .arg("--output")
            .arg(format!("{}={}", output.sink, output.delay_ms));
    }
    let output = command
        .arg("--dry-run")
        .output()
        .map_err(|error| ApiError::internal(format!("could not preview mirror: {error}")))?;
    if !output.status.success() {
        return Err(ApiError::bad_request(format!(
            "mirror preview failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let output = String::from_utf8(output.stdout)
        .map_err(|error| ApiError::internal(format!("mirror preview was not UTF-8: {error}")))?;
    Ok(Json(MirrorPreview { output }))
}

async fn stop_mirror(State(state): State<WebState>) -> Result<Json<UiState>, ApiError> {
    state.stop_mirror()?;
    ui_state(&state)
}

async fn local_host_only(request: Request<Body>, next: Next) -> Response {
    let headers = request.headers();
    let allowed_host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_loopback_authority);
    let allowed_origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_none_or(is_loopback_origin);
    if !allowed_host || !allowed_origin {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

fn is_loopback_authority(value: &str) -> bool {
    let Ok(authority) = Authority::from_str(value) else {
        return false;
    };
    if authority.host().eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = authority.host();
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    IpAddr::from_str(host).is_ok_and(|address| address.is_loopback())
}

fn is_loopback_origin(value: &str) -> bool {
    let Ok(origin) = Uri::from_str(value) else {
        return false;
    };
    origin.scheme_str() == Some("http")
        && origin
            .authority()
            .is_some_and(|authority| is_loopback_authority(authority.as_str()))
}

fn ui_state(state: &WebState) -> Result<Json<UiState>, ApiError> {
    let devices = discover_devices().map_err(ApiError::internal)?;
    let settings = state.settings()?;
    let mirror_running = state.mirror_running()?;
    Ok(Json(UiState {
        microphones: devices.microphones,
        outputs: devices.outputs,
        default_microphone: pactl_output(&["get-default-source"]).ok(),
        selected_microphone: settings.selected_microphone,
        selected_outputs: settings.outputs,
        mirror_running,
        calibration_available: false,
    }))
}

struct DeviceSnapshot {
    microphones: Vec<MicrophoneDevice>,
    outputs: Vec<PlaybackDevice>,
}

fn discover_devices() -> Result<DeviceSnapshot, String> {
    let dump = command_output("pw-dump", &[])?;
    let root: Value = serde_json::from_str(&dump)
        .map_err(|error| format!("could not parse PipeWire JSON: {error}"))?;
    let sinks = parse_pipewire_sinks(&dump)?;
    let sink_rows = pactl_short_nodes("sinks")?;
    let source_rows = pactl_short_nodes("sources")?;
    let sink_status: HashMap<_, _> = sink_rows
        .into_iter()
        .map(|node| (node.name.clone(), node))
        .collect();
    let source_status: HashMap<_, _> = source_rows
        .into_iter()
        .filter(|node| !node.name.ends_with(".monitor"))
        .map(|node| (node.name.clone(), node))
        .collect();

    let outputs = sinks
        .into_iter()
        .map(|sink| playback_device(sink, &root, &sink_status))
        .collect();
    let microphones = source_status
        .values()
        .map(|source| {
            let (description, node_state) = node_metadata(&root, &source.name);
            MicrophoneDevice {
                name: source.name.clone(),
                description,
                sample_spec: source.sample_spec.clone(),
                state: if source.state.is_empty() {
                    node_state
                } else {
                    source.state.clone()
                },
            }
        })
        .collect();

    Ok(DeviceSnapshot {
        microphones,
        outputs,
    })
}

fn playback_device(
    sink: SinkCapabilities,
    root: &Value,
    status: &HashMap<String, PactlNode>,
) -> PlaybackDevice {
    let (description, node_state) = node_metadata(root, &sink.name);
    let status = status.get(&sink.name);
    let stereo_error = validate_receiver_sink(&sink, AudioLayout::Stereo).err();
    let transport = transport_hint(&sink);
    PlaybackDevice {
        name: sink.name.clone(),
        description,
        sample_spec: status
            .map(|status| status.sample_spec.clone())
            .unwrap_or_else(|| "format unavailable".to_owned()),
        state: status
            .map(|status| status.state.clone())
            .filter(|state| !state.is_empty())
            .unwrap_or(node_state),
        channels: sink.channels,
        positions: sink.positions,
        transport,
        stereo_supported: stereo_error.is_none(),
        stereo_error,
    }
}

fn transport_hint(sink: &SinkCapabilities) -> String {
    if sink.device_api.as_deref() == Some("bluez5") || sink.name.starts_with("bluez_output.") {
        "Bluetooth A2DP; codec may be SBC".to_owned()
    } else if sink.device_api.as_deref() == Some("alsa")
        && sink.name.to_ascii_lowercase().contains("spdif")
    {
        "S/PDIF; stereo PCM".to_owned()
    } else {
        "PipeWire output".to_owned()
    }
}

fn node_metadata(root: &Value, name: &str) -> (String, String) {
    let entry = root.as_array().and_then(|entries| {
        entries.iter().find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Node")
                && entry
                    .get("info")
                    .and_then(|info| info.get("props"))
                    .and_then(|props| props.get("node.name"))
                    .and_then(Value::as_str)
                    == Some(name)
        })
    });
    let description = entry
        .and_then(|entry| entry.get("info"))
        .and_then(|info| info.get("props"))
        .and_then(|props| props.get("node.description"))
        .and_then(Value::as_str)
        .filter(|description| !description.is_empty())
        .unwrap_or(name)
        .to_owned();
    let state = entry
        .and_then(|entry| entry.get("info"))
        .and_then(|info| info.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    (description, state)
}

struct PactlNode {
    name: String,
    sample_spec: String,
    state: String,
}

fn pactl_short_nodes(kind: &str) -> Result<Vec<PactlNode>, String> {
    let output = command_output("pactl", &["list", "short", kind])?;
    Ok(pactl_short_nodes_from_text(&output))
}

fn pactl_short_nodes_from_text(output: &str) -> Vec<PactlNode> {
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            (fields.len() >= 5).then(|| PactlNode {
                name: fields[1].to_owned(),
                sample_spec: fields[3].to_owned(),
                state: fields[4].to_owned(),
            })
        })
        .collect()
}

fn command_output(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} returned non-UTF-8 output: {error}"))
}

fn validate_settings(
    settings: &WebSettings,
    devices: &DeviceSnapshot,
    require_two_outputs: bool,
) -> Result<(), ApiError> {
    if let Some(microphone) = &settings.selected_microphone
        && !devices
            .microphones
            .iter()
            .any(|device| device.name == *microphone)
    {
        return Err(ApiError::bad_request(format!(
            "microphone {microphone:?} is not currently available"
        )));
    }
    if require_two_outputs && settings.outputs.len() < 2 {
        return Err(ApiError::bad_request(
            "select at least two stereo playback outputs",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for selected in &settings.outputs {
        if !seen.insert(selected.sink.as_str()) {
            return Err(ApiError::bad_request(format!(
                "output {:?} is selected more than once",
                selected.sink
            )));
        }
        if selected.delay_ms > MAX_OUTPUT_DELAY_MS {
            return Err(ApiError::bad_request(format!(
                "delay for {:?} must be between 0 and {MAX_OUTPUT_DELAY_MS} ms",
                selected.sink
            )));
        }
        let Some(device) = devices
            .outputs
            .iter()
            .find(|device| device.name == selected.sink)
        else {
            return Err(ApiError::bad_request(format!(
                "output {:?} is not currently available",
                selected.sink
            )));
        };
        if !device.stereo_supported {
            return Err(ApiError::bad_request(format!(
                "output {:?} does not support stereo FL/FR: {}",
                selected.sink,
                device
                    .stereo_error
                    .as_deref()
                    .unwrap_or("unknown channel map")
            )));
        }
    }
    Ok(())
}

fn pactl_output(args: &[&str]) -> Result<String, String> {
    command_output("pactl", args).map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_snapshot() -> DeviceSnapshot {
        let output = |name: &str| PlaybackDevice {
            name: name.to_owned(),
            description: name.to_owned(),
            sample_spec: "s16le 2ch 48000Hz".to_owned(),
            state: "SUSPENDED".to_owned(),
            channels: 2,
            positions: vec!["FL".to_owned(), "FR".to_owned()],
            transport: "PipeWire output".to_owned(),
            stereo_supported: true,
            stereo_error: None,
        };
        DeviceSnapshot {
            microphones: vec![MicrophoneDevice {
                name: "test-microphone".to_owned(),
                description: "Test microphone".to_owned(),
                sample_spec: "s16le 2ch 48000Hz".to_owned(),
                state: "SUSPENDED".to_owned(),
            }],
            outputs: vec![output("test-output-a"), output("test-output-b")],
        }
    }

    fn test_settings() -> WebSettings {
        WebSettings {
            selected_microphone: Some("test-microphone".to_owned()),
            outputs: vec![
                SelectedOutput {
                    sink: "test-output-a".to_owned(),
                    delay_ms: 0,
                },
                SelectedOutput {
                    sink: "test-output-b".to_owned(),
                    delay_ms: 23,
                },
            ],
        }
    }

    #[test]
    fn loopback_authority_allows_local_hosts_and_rejects_other_hosts() {
        assert!(is_loopback_authority("localhost:8765"));
        assert!(is_loopback_authority("127.0.0.1:8765"));
        assert!(is_loopback_authority("[::1]:8765"));
        assert!(!is_loopback_authority("example.com:8765"));
        assert!(!is_loopback_authority("localhost.example.com:8765"));
    }

    #[test]
    fn loopback_origin_requires_http_and_a_local_authority() {
        assert!(is_loopback_origin("http://localhost:8765"));
        assert!(is_loopback_origin("http://127.0.0.1:8765"));
        assert!(!is_loopback_origin("https://localhost:8765"));
        assert!(!is_loopback_origin("http://example.com:8765"));
    }

    #[test]
    fn pactl_source_parser_handles_tab_columns_and_ignores_monitor_nodes() {
        let text = concat!(
            "10\talsa_input.mic\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED\n",
            "11\talsa_output.speaker.monitor\tPipeWire\ts16le 2ch 48000Hz\tSUSPENDED\n"
        );
        let rows = pactl_short_nodes_from_text(text);
        let sources: Vec<_> = rows
            .into_iter()
            .filter(|node| !node.name.ends_with(".monitor"))
            .collect();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "alsa_input.mic");
        assert_eq!(sources[0].sample_spec, "s16le 2ch 48000Hz");
    }

    #[test]
    fn settings_accept_available_microphone_and_two_stereo_outputs() {
        assert!(validate_settings(&test_settings(), &test_snapshot(), true).is_ok());
    }

    #[test]
    fn settings_reject_unavailable_devices_duplicate_sinks_and_excessive_delay() {
        let devices = test_snapshot();
        let mut settings = test_settings();
        settings.selected_microphone = Some("missing-microphone".to_owned());
        assert!(validate_settings(&settings, &devices, false).is_err());

        settings = test_settings();
        settings.outputs[1].sink = "test-output-a".to_owned();
        assert!(validate_settings(&settings, &devices, false).is_err());

        settings = test_settings();
        settings.outputs[1].delay_ms = MAX_OUTPUT_DELAY_MS + 1;
        assert!(validate_settings(&settings, &devices, false).is_err());

        settings = test_settings();
        settings.outputs[1].delay_ms = MAX_OUTPUT_DELAY_MS;
        assert!(validate_settings(&settings, &devices, false).is_ok());

        settings = test_settings();
        settings.outputs[1].sink = "missing-output".to_owned();
        assert!(validate_settings(&settings, &devices, false).is_err());
    }

    #[test]
    fn one_stereo_output_is_valid_for_settings_but_not_for_mirroring() {
        let devices = test_snapshot();
        let mut settings = test_settings();
        settings.outputs.pop();
        assert!(validate_settings(&settings, &devices, false).is_ok());
        assert!(validate_settings(&settings, &devices, true).is_err());
    }
}
