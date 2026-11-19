use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
        .args(args)
        .output()
        .expect("q900-bridge binary should start")
}

#[test]
fn sender_dry_run_shows_eight_channel_pcm_plan() {
    let output = run_cli(&[
        "sender",
        "--layout",
        "7.1",
        "--local-sink",
        "alsa_output.test",
        "--dry-run",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("7.1-channel s24be/48000 Hz RTP"));
    assert!(stdout.contains("channels=8"));
    assert!(stdout.contains("side-left,side-right,rear-left,rear-right"));
}

#[test]
fn sender_refuses_bluetooth_sink_that_may_use_sbc() {
    let output = run_cli(&["sender", "--local-sink", "bluez_output.q900"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Bluetooth"));
    assert!(stderr.contains("SBC"));
}

#[test]
fn sender_rejects_unrequested_stereo_layout() {
    let output = run_cli(&["sender", "--layout", "stereo", "--dry-run"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("choose 5.1 or 7.1"));
}

#[test]
fn sender_rejects_zero_udp_port() {
    let output = run_cli(&[
        "sender",
        "--port",
        "0",
        "--local-sink",
        "alsa_output.test",
        "--dry-run",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("UDP port must be between 1 and 65535"));
}

#[test]
fn sender_rejects_zero_latency() {
    let output = run_cli(&[
        "sender",
        "--latency-ms",
        "0",
        "--local-sink",
        "alsa_output.test",
        "--dry-run",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("latency must be between 1 and 2000 ms"));
}

#[test]
fn sender_rejects_invalid_local_sink_name() {
    let output = run_cli(&["sender", "--local-sink", "alsa output.test", "--dry-run"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid PipeWire sink name"));
}

#[test]
fn receiver_requires_an_explicit_output_sink() {
    let output = run_cli(&["receiver", "--dry-run"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("receiver requires --sink"));
}

#[test]
fn sender_restores_default_and_unloads_modules_after_sigint() {
    let tools = FakeAudioTools::new();
    let mut child = tools.launch(&[
        "sender",
        "--layout",
        "5.1",
        "--destination-ip",
        "127.0.0.1",
        "--local-sink",
        "test_local_sink",
    ]);
    let bus = wait_for_prefix(&tools.default_file, "q900_bridge_");
    thread::sleep(Duration::from_millis(150));
    stop_with_sigint(&mut child);

    assert_eq!(
        fs::read_to_string(&tools.default_file).unwrap(),
        "original\n"
    );
    let log = fs::read_to_string(&tools.log_file).unwrap();
    assert!(log.contains(&format!("set-default-sink {bus}")));
    assert!(log.contains("set-default-sink original"));
    assert_eq!(log.matches("unload-module 100").count(), 3);
}

#[test]
fn receiver_loads_rtp_module_and_unloads_it_after_sigint() {
    let tools = FakeAudioTools::new();
    let mut child = tools.launch(&["receiver", "--sink", "test_hdmi", "--layout", "5.1"]);
    wait_for_log(&tools.log_file, "load-module module-rtp-recv");
    thread::sleep(Duration::from_millis(150));
    stop_with_sigint(&mut child);

    let log = fs::read_to_string(&tools.log_file).unwrap();
    assert!(log.contains("sink=test_hdmi"));
    assert!(log.contains("unload-module 100"));
}

fn stop_with_sigint(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    let signal = Command::new("kill")
        .args(["-INT", &pid])
        .status()
        .expect("kill command should be available");
    assert!(signal.success());
    assert!(child.wait().expect("child should exit").success());
}

fn wait_for_prefix(path: &std::path::Path, prefix: &str) -> String {
    for _ in 0..200 {
        if let Ok(value) = fs::read_to_string(path)
            && value.starts_with(prefix)
        {
            return value.trim().to_owned();
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {} to start", path.display());
}

fn wait_for_log(path: &std::path::Path, expected: &str) {
    for _ in 0..200 {
        if fs::read_to_string(path)
            .map(|log| log.contains(expected))
            .unwrap_or(false)
        {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {expected:?} in {}", path.display());
}

struct FakeAudioTools {
    directory: PathBuf,
    default_file: PathBuf,
    log_file: PathBuf,
}

impl FakeAudioTools {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("q900-cli-test-{}-{nonce}", std::process::id()));
        let bin = directory.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let default_file = directory.join("default-sink");
        let log_file = directory.join("pactl.log");
        write_executable(
            &bin.join("pactl"),
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$Q900_FAKE_PACTL_LOG"
case "$1" in
  get-default-sink)
    if [ -f "$Q900_FAKE_DEFAULT" ]; then cat "$Q900_FAKE_DEFAULT"; else printf 'original\n'; fi
    ;;
  list)
    if [ "$2" = "short" ] && [ "$3" = "sinks" ]; then printf '1 original\n'; fi
    ;;
  load-module) printf '100\n' ;;
  set-default-sink) printf '%s\n' "$2" > "$Q900_FAKE_DEFAULT" ;;
  unload-module|move-sink-input) : ;;
  *) exit 2 ;;
esac
"##,
        );
        write_executable(
            &bin.join("pw-dump"),
            r##"#!/bin/sh
cat <<'JSON'
[{"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"test_hdmi","audio.channels":6,"audio.position":"[ FL, FR, FC, LFE, RL, RR ]"}}}]
JSON
"##,
        );

        Self {
            directory,
            default_file,
            log_file,
        }
    }

    fn launch(&self, args: &[&str]) -> std::process::Child {
        let bin = self.directory.join("bin");
        let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());
        Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
            .args(args)
            .env("PATH", path)
            .env("Q900_FAKE_DEFAULT", &self.default_file)
            .env("Q900_FAKE_PACTL_LOG", &self.log_file)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("q900-bridge process should start")
    }
}

impl Drop for FakeAudioTools {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn write_executable(path: &std::path::Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}
