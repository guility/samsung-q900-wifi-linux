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
fn help_is_available_without_arguments_or_with_either_help_flag() {
    for args in [&[][..], &["--help"][..], &["-h"][..]] {
        let output = run_cli(args);
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout).unwrap().contains("Usage:"));
    }
}

#[test]
fn sender_dry_run_shows_eight_channel_pcm_plan() {
    let output = run_cli(&[
        "sender",
        "--layout",
        "7.1",
        "--local-sink",
        "alsa_output.test",
        "--local-delay-ms",
        "37",
        "--dry-run",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("7.1-channel s24be/48000 Hz RTP"));
    assert!(stdout.contains("channels=8"));
    assert!(stdout.contains("side-left,side-right,rear-left,rear-right"));
    assert!(stdout.contains("pw-loopback"));
    assert!(stdout.contains("--delay 0.037"));
    assert!(stdout.contains("--capture-props {\"stream.capture.sink\":true}"));
    assert!(stdout.contains("--channel-map [ FL FR FC LFE SL SR RL RR ]"));
}

#[test]
fn sender_delay_defaults_to_zero_and_accepts_the_two_second_limit() {
    let default = run_cli(&["sender", "--local-sink", "alsa_output.test", "--dry-run"]);
    assert!(default.status.success());
    assert!(
        String::from_utf8(default.stdout)
            .unwrap()
            .contains("--delay 0.000")
    );

    let upper_bound = run_cli(&[
        "sender",
        "--local-sink",
        "alsa_output.test",
        "--local-delay-ms",
        "2000",
        "--dry-run",
    ]);
    assert!(upper_bound.status.success());
    assert!(
        String::from_utf8(upper_bound.stdout)
            .unwrap()
            .contains("--delay 2.000")
    );
}

#[test]
fn receiver_dry_run_applies_fixed_output_delay() {
    let tools = FakeAudioTools::new();
    let output = tools.run(&[
        "receiver",
        "--sink",
        "test_hdmi",
        "--layout",
        "5.1",
        "--output-delay-ms",
        "125",
        "--dry-run",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pw-loopback"));
    assert!(stdout.contains("--delay 0.125"));
    assert!(stdout.contains("--playback test_hdmi"));
    assert!(stdout.contains("--capture-props {\"stream.capture.sink\":true}"));
}

#[test]
fn lpcm_check_dry_run_lists_51_channels_without_changing_default_routes() {
    let tools = FakeAudioTools::new();
    let output = tools.run(&[
        "lpcm-check",
        "--sink",
        "test_hdmi",
        "--layout",
        "5.1",
        "--dry-run",
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("5.1-channel 24-bit/48 kHz test tones"));
    assert!(stdout.contains("FL (front left, 440 Hz)"));
    assert!(stdout.contains("LFE (subwoofer/LFE, 80 Hz)"));
    assert!(stdout.contains("The default sink and existing application routes are not changed."));
    assert!(!tools.log_file.exists());
    assert!(!tools.loopback_pid_file.exists());
}

#[test]
fn lpcm_check_refuses_71_on_a_six_channel_sink() {
    let tools = FakeAudioTools::new();
    let output = tools.run(&[
        "lpcm-check",
        "--sink",
        "test_hdmi",
        "--layout",
        "7.1",
        "--dry-run",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("exposes 6 channel(s); 7.1 requires at least 8"));
    assert!(!tools.log_file.exists());
}

#[test]
fn lpcm_check_plays_native_24_bit_pcm_and_restores_temporary_pipewire_nodes() {
    let tools = FakeAudioTools::new();
    let output = tools.run(&[
        "lpcm-check",
        "--sink",
        "test_hdmi",
        "--layout",
        "5.1",
        "--tone-ms",
        "200",
        "--gap-ms",
        "0",
    ]);

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let playback_args = fs::read_to_string(&tools.pw_cat_log_file).unwrap();
    assert!(playback_args.contains("--playback --raw --target q900_lpcm_test_"));
    assert!(playback_args.contains("--rate 48000 --channels 6"));
    assert!(playback_args.contains("--channel-map FL,FR,FC,LFE,RL,RR"));
    assert!(playback_args.contains("--format s24 -"));

    let pcm = fs::read(&tools.pw_cat_pcm_file).unwrap();
    assert_eq!(pcm.len(), 6 * 200 * 48 * 6 * 3);
    let pactl_log = fs::read_to_string(&tools.log_file).unwrap();
    assert!(pactl_log.contains("load-module module-null-sink"));
    assert!(pactl_log.contains("unload-module 100"));
    assert!(!pactl_log.contains("set-default-sink"));
    assert!(!tools.default_file.exists());
    assert_eq!(
        fs::read_to_string(tools.directory.join("input-sink")).unwrap(),
        "original\n"
    );
    assert_loopback_process_stopped(&tools.loopback_pid_file);
}

#[test]
fn lpcm_check_rejects_out_of_range_tone_and_gap_durations() {
    let outputs = [
        run_cli(&[
            "lpcm-check",
            "--sink",
            "test_hdmi",
            "--tone-ms",
            "199",
            "--dry-run",
        ]),
        run_cli(&[
            "lpcm-check",
            "--sink",
            "test_hdmi",
            "--tone-ms",
            "3001",
            "--dry-run",
        ]),
        run_cli(&[
            "lpcm-check",
            "--sink",
            "test_hdmi",
            "--gap-ms",
            "2001",
            "--dry-run",
        ]),
    ];
    for output in &outputs {
        assert!(!output.status.success());
    }
    assert!(
        String::from_utf8_lossy(&outputs[0].stderr)
            .contains("--tone-ms must be between 200 and 3000 ms")
    );
    assert!(
        String::from_utf8_lossy(&outputs[1].stderr)
            .contains("--tone-ms must be between 200 and 3000 ms")
    );
    assert!(
        String::from_utf8_lossy(&outputs[2].stderr)
            .contains("--gap-ms must be between 0 and 2000 ms")
    );
}

#[test]
fn fixed_delay_options_reject_values_above_two_seconds() {
    let outputs = [
        run_cli(&[
            "sender",
            "--local-sink",
            "alsa_output.test",
            "--local-delay-ms",
            "2001",
            "--dry-run",
        ]),
        run_cli(&[
            "receiver",
            "--sink",
            "test_sink",
            "--output-delay-ms",
            "2001",
            "--dry-run",
        ]),
    ];
    for output in outputs {
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("delay must be between 0 and 2000 ms"));
    }
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
fn sync_report_reads_acoustic_observations_from_csv() {
    let path = env::temp_dir().join(format!(
        "q900-sync-{}-{}.csv",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        "elapsed_s,local_ms,q900_ms\n0,0,8\n60,0,8\n120,0,8\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
        .args(["sync-report", "--input"])
        .arg(&path)
        .output()
        .expect("q900-bridge should start");
    let _ = fs::remove_file(path);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Observations: 3"));
    assert!(stdout.contains("Q900T offset at start: 8.000 ms"));
    assert!(stdout.contains("Drift: 0.000 ms/min"));
    assert!(stdout.contains("Median |skew|: 8.000 ms"));
    assert!(stdout.contains("P95 |skew|: 8.000 ms"));
    assert!(stdout.contains("delay local output by 8.000 ms"));
}

#[test]
fn sync_report_zero_offset_recommends_no_fixed_delay() {
    let path = env::temp_dir().join(format!(
        "q900-sync-aligned-{}-{}.csv",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(
        &path,
        "elapsed_s,local_ms,q900_ms\n0,0,0\n60,0,0\n120,0,0\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
        .args(["sync-report", "--input"])
        .arg(&path)
        .output()
        .expect("q900-bridge should start");
    let _ = fs::remove_file(path);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("outputs align at start"));
    assert!(stdout.contains("Recommended compensation: no fixed delay"));
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
    wait_for_prefix(&tools.loopback_pid_file, "");
    wait_for_log(&tools.log_file, &format!("move-sink-input 10 {bus}"));
    thread::sleep(Duration::from_millis(150));
    stop_with_sigint(&mut child);

    assert_eq!(
        fs::read_to_string(&tools.default_file).unwrap(),
        "original\n"
    );
    let log = fs::read_to_string(&tools.log_file).unwrap();
    assert!(log.contains(&format!("set-default-sink {bus}")));
    assert!(log.contains("set-default-sink original"));
    assert!(log.contains("move-sink-input 10 original"));
    assert_eq!(log.matches("unload-module 100").count(), 2);
    assert_loopback_process_stopped(&tools.loopback_pid_file);
}

#[test]
fn sender_ignores_a_stream_that_disappears_during_routing() {
    let tools = FakeAudioTools::new_with_missing_move_once();
    let mut child = tools.launch(&[
        "sender",
        "--layout",
        "5.1",
        "--destination-ip",
        "127.0.0.1",
        "--local-sink",
        "test_local_sink",
    ]);
    wait_for_log(&tools.log_file, "move-sink-input 10 q900_bridge_");
    thread::sleep(Duration::from_millis(150));
    stop_with_sigint(&mut child);

    let log = fs::read_to_string(&tools.log_file).unwrap();
    assert!(log.contains("set-default-sink original"));
    assert_eq!(log.matches("unload-module 100").count(), 2);
    assert_loopback_process_stopped(&tools.loopback_pid_file);
}

#[test]
fn receiver_loads_rtp_module_and_unloads_it_after_sigint() {
    let tools = FakeAudioTools::new();
    let mut child = tools.launch(&["receiver", "--sink", "test_hdmi", "--layout", "5.1"]);
    wait_for_log(&tools.log_file, "load-module module-rtp-recv");
    wait_for_prefix(&tools.loopback_pid_file, "");
    thread::sleep(Duration::from_millis(150));
    stop_with_sigint(&mut child);

    let log = fs::read_to_string(&tools.log_file).unwrap();
    assert_eq!(log.matches("unload-module 100").count(), 2);
    assert!(log.contains("unload-module 100"));
    assert_loopback_process_stopped(&tools.loopback_pid_file);
}

fn assert_loopback_process_stopped(path: &std::path::Path) {
    let pid = fs::read_to_string(path).unwrap();
    let status = Command::new("kill")
        .args(["-0", pid.trim()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill command should be available");
    assert!(
        !status.success(),
        "pw-loopback process {} is still running",
        pid.trim()
    );
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
    loopback_log_file: PathBuf,
    loopback_pid_file: PathBuf,
    pw_cat_log_file: PathBuf,
    pw_cat_pcm_file: PathBuf,
    missing_move_once: bool,
}

impl FakeAudioTools {
    fn new() -> Self {
        Self::create(false)
    }

    fn new_with_missing_move_once() -> Self {
        Self::create(true)
    }

    fn create(missing_move_once: bool) -> Self {
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
        let loopback_log_file = directory.join("pw-loopback.log");
        let loopback_pid_file = directory.join("pw-loopback.pid");
        let pw_cat_log_file = directory.join("pw-cat.log");
        let pw_cat_pcm_file = directory.join("pw-cat.pcm");
        fs::write(directory.join("input-sink"), "original\n").unwrap();
        write_executable(
            &bin.join("pactl"),
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$Q900_FAKE_PACTL_LOG"
case "$1" in
  get-default-sink)
    if [ -f "$Q900_FAKE_DEFAULT" ]; then cat "$Q900_FAKE_DEFAULT"; else printf 'original\n'; fi
    ;;
  list)
    if [ "$2" = "short" ] && [ "$3" = "sinks" ]; then
      printf '1 original\n'
      if [ -f "$Q900_FAKE_BUS" ]; then printf '100 '; cat "$Q900_FAKE_BUS"; fi
    fi
    if [ "$2" = "short" ] && [ "$3" = "sink-inputs" ]; then
      if [ -f "$Q900_FAKE_INPUT_SINK" ]; then
        input_sink=$(cat "$Q900_FAKE_INPUT_SINK")
        if [ "$input_sink" = "original" ]; then printf '10 1\n'; fi
        if [ "$input_sink" != "original" ] && [ -n "$input_sink" ]; then printf '10 100\n'; fi
      fi
    fi
    ;;
  load-module) printf '100\n' ;;
  set-default-sink)
    printf '%s\n' "$2" > "$Q900_FAKE_DEFAULT"
    if [ "$2" != "original" ]; then printf '%s\n' "$2" > "$Q900_FAKE_BUS"; fi
    ;;
  move-sink-input)
    if [ "$Q900_FAKE_MISSING_MOVE_ONCE" = "1" ] && [ ! -f "$Q900_FAKE_MISSING_MOVE_MARKER" ]; then
      : > "$Q900_FAKE_MISSING_MOVE_MARKER"
      : > "$Q900_FAKE_INPUT_SINK"
      printf 'No such entity\n' >&2
      exit 1
    fi
    printf '%s\n' "$3" > "$Q900_FAKE_INPUT_SINK"
    if [ "$3" != "original" ]; then printf '%s\n' "$3" > "$Q900_FAKE_BUS"; fi
    ;;
  unload-module) : ;;
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
        write_executable(
            &bin.join("pw-loopback"),
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$Q900_FAKE_PW_LOOPBACK_LOG"
printf '%s\n' "$$" > "$Q900_FAKE_PW_LOOPBACK_PID"
exec /usr/bin/sleep 60
"##,
        );
        write_executable(
            &bin.join("pw-cat"),
            r##"#!/bin/sh
printf '%s\n' "$*" >> "$Q900_FAKE_PW_CAT_LOG"
cat > "$Q900_FAKE_PW_CAT_PCM"
"##,
        );

        Self {
            directory,
            default_file,
            log_file,
            loopback_log_file,
            loopback_pid_file,
            pw_cat_log_file,
            pw_cat_pcm_file,
            missing_move_once,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let bin = self.directory.join("bin");
        let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());
        Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
            .args(args)
            .env("PATH", path)
            .env("Q900_FAKE_DEFAULT", &self.default_file)
            .env("Q900_FAKE_PACTL_LOG", &self.log_file)
            .env("Q900_FAKE_PW_LOOPBACK_LOG", &self.loopback_log_file)
            .env("Q900_FAKE_PW_LOOPBACK_PID", &self.loopback_pid_file)
            .env("Q900_FAKE_PW_CAT_LOG", &self.pw_cat_log_file)
            .env("Q900_FAKE_PW_CAT_PCM", &self.pw_cat_pcm_file)
            .env("Q900_FAKE_INPUT_SINK", self.directory.join("input-sink"))
            .env("Q900_FAKE_BUS", self.directory.join("bus-sink"))
            .env(
                "Q900_FAKE_MISSING_MOVE_MARKER",
                self.directory.join("missing-move-seen"),
            )
            .env(
                "Q900_FAKE_MISSING_MOVE_ONCE",
                if self.missing_move_once { "1" } else { "0" },
            )
            .output()
            .expect("q900-bridge process should start")
    }

    fn launch(&self, args: &[&str]) -> std::process::Child {
        let bin = self.directory.join("bin");
        let path = format!("{}:{}", bin.display(), env::var("PATH").unwrap_or_default());
        Command::new(env!("CARGO_BIN_EXE_q900-bridge"))
            .args(args)
            .env("PATH", path)
            .env("Q900_FAKE_DEFAULT", &self.default_file)
            .env("Q900_FAKE_PACTL_LOG", &self.log_file)
            .env("Q900_FAKE_PW_LOOPBACK_LOG", &self.loopback_log_file)
            .env("Q900_FAKE_PW_LOOPBACK_PID", &self.loopback_pid_file)
            .env("Q900_FAKE_PW_CAT_LOG", &self.pw_cat_log_file)
            .env("Q900_FAKE_PW_CAT_PCM", &self.pw_cat_pcm_file)
            .env("Q900_FAKE_INPUT_SINK", self.directory.join("input-sink"))
            .env("Q900_FAKE_BUS", self.directory.join("bus-sink"))
            .env(
                "Q900_FAKE_MISSING_MOVE_MARKER",
                self.directory.join("missing-move-seen"),
            )
            .env(
                "Q900_FAKE_MISSING_MOVE_ONCE",
                if self.missing_move_once { "1" } else { "0" },
            )
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
