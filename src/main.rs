use q900_bridge::calibration::{self, calibration_plan};
use q900_bridge::lpcm::{channel_tones, write_channel_identification_tones};
use q900_bridge::sync::{OutputPath, analyze_sync, parse_sync_csv};
use q900_bridge::{
    AudioLayout, LoopbackSpec, MirrorConfig, MirrorPlan, ModuleSpec, OutputRoute, ReceiverConfig,
    SenderConfig, SinkCapabilities, has_auxiliary_channel_map, mirror_plan, parse_pipewire_sinks,
    receiver_plan, sender_plan, validate_aux_hdmi_route, validate_receiver_sink,
};
use std::collections::HashMap;
use std::env;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod web;

const DEFAULT_DESTINATION: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 56);
const DEFAULT_PORT: u16 = 5004;
const DEFAULT_LATENCY_MS: u16 = 100;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("q900-bridge: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(());
    }
    match args[0].as_str() {
        "mirror" => run_mirror(parse_mirror_args(&args[1..])?),
        "sender" => run_sender(parse_sender_args(&args[1..])?),
        "receiver" => run_receiver(parse_receiver_args(&args[1..])?),
        "lpcm-check" => run_lpcm_check(parse_lpcm_check_args(&args[1..])?),
        "sync-report" => run_sync_report(&args[1..]),
        "web" => run_web(&args[1..]),
        "calibrate" => run_calibration(parse_calibration_args(&args[1..])?),
        other => Err(format!(
            "unknown command {other:?}; expected mirror, sender, receiver, lpcm-check, sync-report, calibrate, or web"
        )),
    }
}

fn run_web(args: &[String]) -> Result<(), String> {
    web::run(parse_web_port(args)?)
}

fn parse_web_port(args: &[String]) -> Result<u16, String> {
    let options = parse_options(args, &["--port"], &[])?;
    match options.value("--port") {
        Some(value) => value
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or_else(|| "--port must be an integer between 1 and 65535".to_owned()),
        None => Ok(8765),
    }
}

fn run_sync_report(args: &[String]) -> Result<(), String> {
    let options = parse_options(args, &["--input"], &[])?;
    let path = options
        .value("--input")
        .ok_or_else(|| "sync-report requires --input CSV_FILE".to_owned())?;
    let csv = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read sync observations {path:?}: {error}"))?;
    let observations = parse_sync_csv(&csv)?;
    let report = analyze_sync(&observations)?;

    let offset_description = if report.q900_offset_at_start_ms > 0.0 {
        "Q900T arrives later"
    } else if report.q900_offset_at_start_ms < 0.0 {
        "local output arrives later"
    } else {
        "outputs align at start"
    };
    println!("Observations: {}", report.observations);
    println!(
        "Q900T offset at start: {:.3} ms ({offset_description})",
        report.q900_offset_at_start_ms
    );
    println!("Drift: {:.3} ms/min", report.drift_ms_per_minute);
    println!("Median |skew|: {:.3} ms", report.median_absolute_skew_ms);
    println!("P95 |skew|: {:.3} ms", report.p95_absolute_skew_ms);
    println!("Max |skew|: {:.3} ms", report.max_absolute_skew_ms);
    println!("P95 residual jitter: {:.3} ms", report.p95_jitter_ms);
    println!("Max residual jitter: {:.3} ms", report.max_jitter_ms);
    match report.delay_earlier_output {
        Some(OutputPath::Local) => println!(
            "Recommended compensation: delay the earlier output by {:.3} ms; set it on that `--output SINK=DELAY_MS` route",
            report.recommended_delay_ms
        ),
        Some(OutputPath::Q900T) => println!(
            "Recommended compensation: delay the Q900T output by {:.3} ms; set it on that `--output SINK=DELAY_MS` route",
            report.recommended_delay_ms
        ),
        None => println!("Recommended compensation: no fixed delay"),
    }
    println!("This report covers acoustic audio-to-audio timing only.");
    Ok(())
}

struct SenderArgs {
    config: SenderConfig,
    dry_run: bool,
}

struct MirrorArgs {
    config: MirrorConfig,
    dry_run: bool,
}

struct ReceiverArgs {
    config: ReceiverConfig,
    dry_run: bool,
}

struct LpcmCheckArgs {
    sink: String,
    layout: AudioLayout,
    tone_ms: u32,
    gap_ms: u32,
    dry_run: bool,
}

struct CalibrationArgs {
    microphone: String,
    outputs: Vec<String>,
    dry_run: bool,
    json: bool,
}

fn parse_calibration_args(args: &[String]) -> Result<CalibrationArgs, String> {
    let mut microphone = None;
    let mut outputs = Vec::new();
    let mut dry_run = false;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--microphone" => {
                if microphone.is_some() {
                    return Err("--microphone may be specified only once".to_owned());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--microphone requires SOURCE_NAME".to_owned())?;
                validate_sink_token(value)?;
                microphone = Some(value.clone());
                index += 2;
            }
            "--output" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--output requires SINK_NAME".to_owned())?;
                validate_sink_token(value)?;
                if outputs.contains(value) {
                    return Err(format!("output {value:?} is selected more than once"));
                }
                outputs.push(value.clone());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    let microphone =
        microphone.ok_or_else(|| "calibrate requires --microphone SOURCE_NAME".to_owned())?;
    if outputs.len() < 2 {
        return Err("calibrate requires at least two --output SINK_NAME values".to_owned());
    }
    if dry_run && json {
        return Err("--dry-run and --json cannot be combined".to_owned());
    }
    Ok(CalibrationArgs {
        microphone,
        outputs,
        dry_run,
        json,
    })
}

fn run_calibration(args: CalibrationArgs) -> Result<(), String> {
    let plan = calibration_plan(&args.microphone, &args.outputs, std::process::id())?;
    if args.dry_run {
        print_calibration_plan(&plan);
        return Ok(());
    }
    validate_calibration_devices(&plan)?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|error| format!("could not install Ctrl-C handler: {error}"))?;

    if !args.json {
        eprintln!(
            "Calibration will play {} low-level coded probes through {} output(s), captured by {} at {} Hz.",
            plan.repetitions,
            plan.outputs.len(),
            plan.microphone,
            plan.sample_rate
        );
        eprintln!(
            "Probe peak is 10% full scale; playback stream gain is 100%, device volume is unchanged."
        );
        eprintln!("No system default route or device volume will be changed.");
    }

    let module_ids = load_modules(&plan.modules)?;
    let result = run_calibration_measurements(&plan, &interrupted);
    let cleanup = unload_modules(&module_ids);
    let report = match (result, cleanup) {
        (Ok(report), Ok(())) => report,
        (Err(error), Ok(())) => return Err(error),
        (Ok(_), Err(error)) => {
            return Err(format!("calibration completed but cleanup failed: {error}"));
        }
        (Err(error), Err(cleanup)) => {
            return Err(format!("{error}; calibration cleanup failed: {cleanup}"));
        }
    };

    if args.json {
        println!("{}", calibration_report_json(&report));
    } else {
        print_calibration_report(&report);
    }
    Ok(())
}

fn print_calibration_plan(plan: &q900_bridge::calibration::CalibrationPlan) {
    println!("Calibration dry run");
    println!("  microphone: {}", plan.microphone);
    println!("  outputs: {}", plan.outputs.join(", "));
    println!(
        "  probe: {} Hz, {} coded channels, 10% peak level",
        plan.sample_rate,
        plan.outputs.len()
    );
    println!("  repetitions: {}", plan.repetitions);
    println!("  route modules:");
    for module in &plan.modules {
        println!("    {} {}", module.name, module.args.join(" "));
    }
    println!("  playback: one synchronized multichannel probe stream");
    println!("  no audio was played; system routes were not changed.");
}

fn validate_calibration_devices(
    plan: &q900_bridge::calibration::CalibrationPlan,
) -> Result<(), String> {
    let sources = pactl_output(&["list", "short", "sources"])?;
    let microphone_available = sources.lines().any(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        fields
            .get(1)
            .is_some_and(|name| *name == plan.microphone && !name.ends_with(".monitor"))
    });
    if !microphone_available {
        return Err(format!(
            "microphone source {:?} is not currently available",
            plan.microphone
        ));
    }
    for output in &plan.outputs {
        let sink = find_pipewire_sink(output)?;
        validate_stereo_sink(&sink, "calibration output")?;
    }
    Ok(())
}

fn run_calibration_measurements(
    plan: &q900_bridge::calibration::CalibrationPlan,
    interrupted: &Arc<AtomicBool>,
) -> Result<calibration::CalibrationReport, String> {
    let directory = TemporaryDirectory::create()?;
    let probes: Vec<_> = (0..plan.outputs.len())
        .map(|index| calibration::generate_probe(plan.sample_rate, index))
        .collect::<Result<_, _>>()?;
    let frame_count = probes.first().map(Vec::len).unwrap_or_default();
    if probes.iter().any(|probe| probe.len() != frame_count) {
        return Err("generated calibration probes have different lengths".to_owned());
    }
    let playback_path = directory.path.join("probes.raw");
    write_interleaved_f32(&playback_path, &probes)?;
    let channel_map = (0..plan.outputs.len())
        .map(|index| format!("aux{index}"))
        .collect::<Vec<_>>()
        .join(",");

    let mut measurements = vec![Vec::with_capacity(plan.repetitions); plan.outputs.len()];
    for repetition in 0..plan.repetitions {
        let capture_path = directory.path.join(format!("capture-{repetition}.raw"));
        let capture = capture_calibration_trial(
            plan,
            &playback_path,
            &capture_path,
            &channel_map,
            interrupted,
        )?;
        let mut failures = Vec::new();
        for (index, (sink, probe)) in plan.outputs.iter().zip(&probes).enumerate() {
            match calibration::estimate_arrival(probe, &capture, 2, plan.sample_rate) {
                Ok(estimate) => measurements[index].push(estimate),
                Err(error) => failures.push(format!("{sink:?}: {error}")),
            }
        }
        if !failures.is_empty() {
            let successful = plan
                .outputs
                .iter()
                .enumerate()
                .filter_map(|(index, sink)| {
                    measurements[index].last().map(|reading| {
                        format!(
                            "{sink:?}: {:.3} ms, confidence {:.3}",
                            reading.arrival_ms, reading.confidence
                        )
                    })
                })
                .collect::<Vec<_>>();
            let successful = if successful.is_empty() {
                String::new()
            } else {
                format!("; detected responses: {}", successful.join(", "))
            };
            return Err(format!(
                "could not measure calibration repetition {}: {}{}",
                repetition + 1,
                failures.join("; "),
                successful
            ));
        }
        if !interrupted.load(Ordering::SeqCst) && repetition + 1 < plan.repetitions {
            thread::sleep(Duration::from_millis(250));
        }
    }
    calibration::build_report(
        &plan.microphone,
        &plan.outputs,
        &measurements,
        plan.sample_rate,
    )
}

fn capture_calibration_trial(
    plan: &q900_bridge::calibration::CalibrationPlan,
    playback_path: &std::path::Path,
    capture_path: &std::path::Path,
    channel_map: &str,
    interrupted: &Arc<AtomicBool>,
) -> Result<Vec<f32>, String> {
    const CAPTURE_MS: u32 = 2_000;
    let capture_samples = plan.sample_rate * CAPTURE_MS / 1000;
    let mut recorder = Command::new("pw-record")
        .args([
            "--raw",
            "--format",
            "f32",
            "--rate",
            &plan.sample_rate.to_string(),
            "--channels",
            "2",
            "--channel-map",
            "stereo",
            "--target",
            &plan.microphone,
            "--latency",
            "40ms",
            "--sample-count",
            &capture_samples.to_string(),
        ])
        .arg(capture_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start PipeWire microphone capture: {error}"))?;

    if let Err(error) = wait_for_start_window(interrupted, Duration::from_millis(500)) {
        terminate_child(&mut recorder);
        return Err(error);
    }

    let mut player = match Command::new("pw-play")
        .args([
            "--raw",
            "--format",
            "f32",
            "--rate",
            &plan.sample_rate.to_string(),
            "--channels",
            &plan.outputs.len().to_string(),
            "--channel-map",
            channel_map,
            "--target",
            &plan.bus_sink,
            "--latency",
            "40ms",
            "--volume",
            "1.0",
        ])
        .arg(playback_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(player) => player,
        Err(error) => {
            terminate_child(&mut recorder);
            return Err(format!("could not start PipeWire probe playback: {error}"));
        }
    };

    let player_status = match wait_for_child(&mut player, "probe playback", interrupted) {
        Ok(status) => status,
        Err(error) => {
            terminate_child(&mut recorder);
            return Err(error);
        }
    };
    if !player_status.success() {
        terminate_child(&mut recorder);
        return Err(format!("pw-play exited with status {player_status}"));
    }
    let record_status = wait_for_child(&mut recorder, "microphone capture", interrupted)?;

    let bytes = std::fs::read(capture_path)
        .map_err(|error| format!("could not read microphone capture: {error}"))?;
    let expected_bytes = capture_samples as usize * 2 * std::mem::size_of::<f32>();
    if bytes.len() != expected_bytes {
        return Err(format!(
            "microphone capture was incomplete: got {} bytes, expected {expected_bytes} (pw-record status {record_status})",
            bytes.len()
        ));
    }
    let (chunks, remainder) = bytes.as_chunks::<4>();
    if !remainder.is_empty() {
        return Err("microphone capture ended in the middle of a sample".to_owned());
    }
    let samples = chunks
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect::<Vec<_>>();
    if samples.len() < plan.sample_rate as usize * 2 {
        return Err(format!(
            "microphone capture was too short ({} samples)",
            samples.len()
        ));
    }
    Ok(samples)
}

fn write_interleaved_f32(path: &std::path::Path, probes: &[Vec<f32>]) -> Result<(), String> {
    let frames = probes.first().map(Vec::len).unwrap_or_default();
    let channels = probes.len();
    let mut bytes = Vec::with_capacity(frames * channels * 4);
    for frame in 0..frames {
        for probe in probes {
            bytes.extend_from_slice(&probe[frame].to_le_bytes());
        }
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("could not write {channels}-channel calibration probe: {error}"))
}

fn wait_for_start_window(interrupted: &AtomicBool, duration: Duration) -> Result<(), String> {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if interrupted.load(Ordering::SeqCst) {
            return Err("calibration was interrupted".to_owned());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn wait_for_child(
    child: &mut Child,
    description: &str,
    interrupted: &AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if interrupted.load(Ordering::SeqCst) {
            terminate_child(child);
            return Err("calibration was interrupted".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                terminate_child(child);
                return Err(format!("could not check {description}: {error}"));
            }
        }
        if Instant::now() >= deadline {
            terminate_child(child);
            return Err(format!("{description} did not finish within 15 seconds"));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn terminate_child(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn create() -> Result<Self, String> {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        for _ in 0..8 {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("system clock is invalid: {error}"))?
                .as_nanos();
            let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "q900-calibration-{}-{timestamp}-{serial}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("could not create calibration workspace: {error}"));
                }
            }
        }
        Err("could not allocate a unique calibration workspace".to_owned())
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn calibration_report_json(report: &calibration::CalibrationReport) -> serde_json::Value {
    serde_json::json!({
        "microphone": report.microphone,
        "sample_rate": report.sample_rate,
        "repetitions": report.repetitions,
        "median_relative_skew_ms": report.median_relative_skew_ms,
        "maximum_relative_skew_ms": report.maximum_relative_skew_ms,
        "outputs": report.outputs.iter().map(|output| serde_json::json!({
            "sink": output.sink,
            "median_arrival_ms": output.median_arrival_ms,
            "minimum_arrival_ms": output.minimum_arrival_ms,
            "maximum_arrival_ms": output.maximum_arrival_ms,
            "confidence": output.confidence,
            "peak_to_sidelobe_ratio": output.peak_to_sidelobe_ratio,
            "delay_ms": output.delay_ms,
            "measurements": output.measurements.iter().map(|reading| serde_json::json!({
                "arrival_ms": reading.arrival_ms,
                "confidence": reading.confidence,
                "peak_to_sidelobe_ratio": reading.peak_to_sidelobe_ratio,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn print_calibration_report(report: &calibration::CalibrationReport) {
    println!(
        "Calibration measurements using {} at {} Hz:",
        report.microphone, report.sample_rate
    );
    println!("  repetitions: {}", report.repetitions);
    println!(
        "  median relative skew: {:.3} ms",
        report.median_relative_skew_ms
    );
    println!(
        "  maximum relative skew: {:.3} ms",
        report.maximum_relative_skew_ms
    );
    for output in &report.outputs {
        println!(
            "  {}: median arrival {:.3} ms (range {:.3}–{:.3}), confidence {:.3}, proposed delay {} ms",
            output.sink,
            output.median_arrival_ms,
            output.minimum_arrival_ms,
            output.maximum_arrival_ms,
            output.confidence,
            output.delay_ms
        );
    }
    println!("Proposed delays are not applied. Review them before updating mirror settings.");
}

fn parse_sender_args(args: &[String]) -> Result<SenderArgs, String> {
    let options = parse_options(
        args,
        &[
            "--layout",
            "--destination-ip",
            "--source-ip",
            "--port",
            "--local-sink",
            "--latency-ms",
            "--local-delay-ms",
        ],
        &["--dry-run"],
    )?;
    let layout = parse_layout(options.value("--layout"))?;
    let destination = parse_destination(options.value("--destination-ip"))?;
    let source_ip = parse_source_ip(options.value("--source-ip"))?;
    let port = parse_port(options.value("--port"))?;
    let local_sink = parse_local_sink(options.value("--local-sink"))?;
    let latency_ms = parse_latency_option(options.value("--latency-ms"))?;
    let local_delay_ms = parse_delay_option(options.value("--local-delay-ms"))?;

    Ok(SenderArgs {
        config: SenderConfig {
            layout,
            destination,
            source_ip,
            port,
            rate: 48_000,
            local_sink,
            latency_ms,
            local_delay_ms,
        },
        dry_run: options.enabled("--dry-run"),
    })
}

fn parse_mirror_args(args: &[String]) -> Result<MirrorArgs, String> {
    let mut outputs = Vec::new();
    let mut latency = None;
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--output requires SINK_NAME[=DELAY_MS]".to_owned())?;
                let (sink, delay) = match value.split_once('=') {
                    Some((sink, delay)) => (sink, parse_delay_option(Some(delay))?),
                    None => (value.as_str(), 0),
                };
                validate_sink_token(sink)?;
                if outputs
                    .iter()
                    .any(|output: &OutputRoute| output.sink == sink)
                {
                    return Err(format!("output sink {sink:?} was specified more than once"));
                }
                outputs.push(OutputRoute {
                    sink: sink.to_owned(),
                    delay_ms: delay,
                });
                index += 2;
            }
            "--latency-ms" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| "--latency-ms requires a value".to_owned())?;
                if latency.replace(value.clone()).is_some() {
                    return Err("--latency-ms may only be specified once".to_owned());
                }
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            flag => return Err(format!("unknown option {flag:?}")),
        }
    }
    if outputs.len() < 2 {
        return Err(
            "mirror requires at least two --output SINK_NAME[=DELAY_MS] entries".to_owned(),
        );
    }
    Ok(MirrorArgs {
        config: MirrorConfig {
            outputs,
            latency_ms: parse_latency_option(latency.as_deref())?,
        },
        dry_run,
    })
}

fn parse_local_sink(value: Option<&str>) -> Result<String, String> {
    let sink = match value {
        Some(sink) => sink.to_owned(),
        None => pactl_output(&["get-default-sink"])?.trim().to_owned(),
    };
    validate_sink_token(&sink)?;
    Ok(sink)
}

fn parse_receiver_args(args: &[String]) -> Result<ReceiverArgs, String> {
    let options = parse_options(
        args,
        &["--layout", "--sink", "--latency-ms", "--output-delay-ms"],
        &["--dry-run"],
    )?;
    let layout = parse_layout(options.value("--layout"))?;
    let sink = options
        .value("--sink")
        .ok_or_else(|| "receiver requires --sink SINK_NAME".to_owned())?
        .to_owned();
    validate_sink_token(&sink)?;
    let latency_ms = parse_latency_option(options.value("--latency-ms"))?;
    let output_delay_ms = parse_delay_option(options.value("--output-delay-ms"))?;

    Ok(ReceiverArgs {
        config: ReceiverConfig {
            layout,
            sink,
            latency_ms,
            output_delay_ms,
        },
        dry_run: options.enabled("--dry-run"),
    })
}

fn parse_lpcm_check_args(args: &[String]) -> Result<LpcmCheckArgs, String> {
    let options = parse_options(
        args,
        &["--sink", "--layout", "--tone-ms", "--gap-ms"],
        &["--dry-run"],
    )?;
    let sink = options
        .value("--sink")
        .ok_or_else(|| "lpcm-check requires --sink SINK_NAME".to_owned())?
        .to_owned();
    validate_sink_token(&sink)?;
    let layout = parse_layout(options.value("--layout"))?;
    let tone_ms = parse_duration_option(options.value("--tone-ms"), "--tone-ms", 800, 200, 3000)?;
    let gap_ms = parse_duration_option(options.value("--gap-ms"), "--gap-ms", 300, 0, 2000)?;
    Ok(LpcmCheckArgs {
        sink,
        layout,
        tone_ms,
        gap_ms,
        dry_run: options.enabled("--dry-run"),
    })
}

fn parse_duration_option(
    value: Option<&str>,
    flag: &str,
    default: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(default);
    };
    let duration = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {flag} value {value:?}"))?;
    if !(minimum..=maximum).contains(&duration) {
        return Err(format!("{flag} must be between {minimum} and {maximum} ms"));
    }
    Ok(duration)
}

struct ParsedOptions(HashMap<String, Option<String>>);

impl ParsedOptions {
    fn value(&self, flag: &str) -> Option<&str> {
        self.0.get(flag).and_then(Option::as_deref)
    }

    fn enabled(&self, flag: &str) -> bool {
        self.0.contains_key(flag)
    }
}

fn parse_options(
    args: &[String],
    value_flags: &[&str],
    switch_flags: &[&str],
) -> Result<ParsedOptions, String> {
    let mut parsed = HashMap::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        if switch_flags.contains(&flag) {
            parsed.insert(flag.to_owned(), None);
            index += 1;
            continue;
        }
        if !value_flags.contains(&flag) {
            return Err(format!("unknown option {flag:?}"));
        }
        let value = args
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("{flag} requires a value"))?;
        parsed.insert(flag.to_owned(), Some(value.clone()));
        index += 2;
    }
    Ok(ParsedOptions(parsed))
}

fn parse_layout(value: Option<&str>) -> Result<AudioLayout, String> {
    value
        .map(AudioLayout::from_str)
        .unwrap_or(Ok(AudioLayout::Surround51))
}

fn parse_destination(value: Option<&str>) -> Result<Ipv4Addr, String> {
    value
        .map(|value| {
            Ipv4Addr::from_str(value).map_err(|_| format!("invalid IPv4 address {value:?}"))
        })
        .unwrap_or(Ok(DEFAULT_DESTINATION))
}

fn parse_source_ip(value: Option<&str>) -> Result<Option<Ipv4Addr>, String> {
    value
        .map(|value| {
            let address = Ipv4Addr::from_str(value)
                .map_err(|_| format!("invalid source IPv4 address {value:?}"))?;
            if address.is_unspecified() || address.is_multicast() || address == Ipv4Addr::BROADCAST
            {
                return Err("source IPv4 address must be a unicast address".to_owned());
            }
            Ok(address)
        })
        .transpose()
}

fn parse_port(value: Option<&str>) -> Result<u16, String> {
    let port = value
        .map(|value| {
            value
                .parse::<u16>()
                .map_err(|_| format!("invalid UDP port {value:?}"))
        })
        .unwrap_or(Ok(DEFAULT_PORT))?;
    if port == 0 {
        return Err("UDP port must be between 1 and 65535".to_owned());
    }
    Ok(port)
}

fn parse_latency_option(value: Option<&str>) -> Result<u16, String> {
    value.map(parse_latency).unwrap_or(Ok(DEFAULT_LATENCY_MS))
}

fn parse_latency(value: &str) -> Result<u16, String> {
    let latency = value
        .parse::<u16>()
        .map_err(|_| format!("invalid latency {value:?}; expected 1 through 2000 ms"))?;
    if !(1..=2000).contains(&latency) {
        return Err("latency must be between 1 and 2000 ms".to_owned());
    }
    Ok(latency)
}

fn parse_delay_option(value: Option<&str>) -> Result<u16, String> {
    let Some(value) = value else {
        return Ok(0);
    };
    let delay = value
        .parse::<u16>()
        .map_err(|_| format!("invalid delay {value:?}; expected 0 through 2000 ms"))?;
    if delay > 2000 {
        return Err("delay must be between 0 and 2000 ms".to_owned());
    }
    Ok(delay)
}

fn validate_sink_token(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    {
        return Err(format!("invalid PipeWire sink name {value:?}"));
    }
    Ok(())
}

fn run_mirror(args: MirrorArgs) -> Result<(), String> {
    let sinks: Vec<SinkCapabilities> = args
        .config
        .outputs
        .iter()
        .map(|output| {
            let sink = find_pipewire_sink(&output.sink)?;
            validate_stereo_sink(&sink, &format!("output {}", output.sink))?;
            Ok(sink)
        })
        .collect::<Result<_, String>>()?;

    let bus = format!("audio_mirror_stereo_{}", std::process::id());
    let plan = mirror_plan(&bus, &args.config);
    if args.dry_run {
        print_mirror_plan(&plan, &args.config);
        for sink in &sinks {
            print_output_transport(sink);
        }
        return Ok(());
    }

    let old_default = pactl_output(&["get-default-sink"])?.trim().to_owned();
    let prior_inputs = current_sink_inputs()?;
    let module_ids = load_modules(&plan.modules)?;
    let mut session = AudioSession {
        module_ids,
        bus_sink: bus.clone(),
        old_default: old_default.clone(),
        prior_inputs,
        loopback_processes: Vec::new(),
    };

    for output in &plan.outputs {
        match spawn_loopback(output) {
            Ok(process) => session.loopback_processes.push(process),
            Err(error) => {
                let _ = session.stop();
                return Err(error);
            }
        }
    }
    if let Err(error) = pactl(&["set-default-sink", &bus]) {
        let _ = session.stop();
        return Err(error);
    }
    if let Err(error) = move_inputs_to_bus(&session) {
        let _ = session.stop();
        return Err(error);
    }

    println!(
        "Mirroring all system audio as stereo to {} outputs.",
        sinks.len()
    );
    for (index, (output, sink)) in args.config.outputs.iter().zip(&sinks).enumerate() {
        println!(
            "  output {}: {} ({} ms added delay)",
            index + 1,
            output.sink,
            output.delay_ms
        );
        print_output_transport(sink);
    }
    println!("Press Ctrl-C to restore the prior route.");
    let wait = wait_until_stopped();
    let cleanup = session.stop();
    wait.and(cleanup)
}

fn validate_stereo_sink(sink: &SinkCapabilities, role: &str) -> Result<(), String> {
    validate_receiver_sink(sink, AudioLayout::Stereo).map_err(|error| format!("{role}: {error}"))
}

fn print_output_transport(sink: &SinkCapabilities) {
    if sink.device_api.as_deref() == Some("bluez5") || sink.name.starts_with("bluez_output.") {
        println!("  transport hint: Bluetooth A2DP; active codec may use SBC.");
    } else if sink.device_api.as_deref() == Some("alsa")
        && sink.name.to_ascii_lowercase().contains("spdif")
    {
        println!("  transport hint: stereo PCM over S/PDIF; confirm the cable and endpoint input.");
    } else {
        println!(
            "  transport hint: selected PipeWire sink; confirm its format and physical connection."
        );
    }
}

fn run_sender(args: SenderArgs) -> Result<(), String> {
    if args.config.local_sink.starts_with("bluez_output.") {
        if args.dry_run {
            eprintln!(
                "warning: this local mirror is Bluetooth; choose a non-Bluetooth local sink for uncompressed audio"
            );
        } else {
            return Err(
                "refusing a Bluetooth local mirror because it may use SBC; choose a non-Bluetooth `--local-sink`".to_owned(),
            );
        }
    }
    let bus = format!("q900_bridge_{}", std::process::id());
    let plan = sender_plan(&bus, &args.config);
    if args.dry_run {
        print_sender_plan(&plan, &args.config);
        return Ok(());
    }

    let old_default = pactl_output(&["get-default-sink"])?.trim().to_owned();
    let prior_inputs = current_sink_inputs()?;
    let module_ids = load_modules(&plan.modules)?;
    let mut session = AudioSession {
        module_ids,
        bus_sink: bus.clone(),
        old_default: old_default.clone(),
        prior_inputs,
        loopback_processes: Vec::new(),
    };

    match spawn_loopback(&plan.local_mirror) {
        Ok(process) => session.loopback_processes.push(process),
        Err(error) => {
            let _ = session.stop();
            return Err(error);
        }
    }

    if let Err(error) = pactl(&["set-default-sink", &bus]) {
        let _ = session.stop();
        return Err(error);
    }
    if let Err(error) = move_inputs_to_bus(&session) {
        let _ = session.stop();
        return Err(error);
    }

    println!(
        "Sending {} ({} channels) 24-bit/48 kHz PCM via RTP to {}:{}; local mirror: {} with {} ms added delay.",
        args.config.layout,
        args.config.layout.channels(),
        args.config.destination,
        args.config.port,
        args.config.local_sink,
        args.config.local_delay_ms
    );
    println!("All system audio is routed through {bus}. Press Ctrl-C to restore the prior route.");
    let wait = wait_until_stopped();
    let cleanup = session.stop();
    wait.and(cleanup)
}

fn run_receiver(args: ReceiverArgs) -> Result<(), String> {
    let sink = find_pipewire_sink(&args.config.sink)?;
    validate_receiver_sink(&sink, args.config.layout).map_err(|error| {
        format!(
            "{error}. Select an active output sink that exposes the requested channel positions, then recheck with `pw-dump`."
        )
    })?;
    if has_auxiliary_channel_map(&sink, args.config.layout) {
        let cards = pactl_output(&["list", "cards"])?;
        validate_aux_hdmi_route(&sink, &cards)?;
    }
    let rtp_sink = format!("q900_rtp_rx_{}", std::process::id());
    let plan = receiver_plan(&args.config, &rtp_sink, &sink);
    if args.dry_run {
        println!(
            "Receiver plan: RTP sink {} -> output {} -> {}.",
            plan.rtp_sink, sink.name, plan.output_sink
        );
        for spec in &plan.modules {
            println!("  {} {}", spec.name, spec.args.join(" "));
        }
        print_loopback(&plan.output_loopback);
        println!(
            "Selected sink {} currently exposes {} channels: {}.",
            sink.name,
            sink.channels,
            sink.positions.join(", ")
        );
        return Ok(());
    }

    let ids = load_modules(&plan.modules)?;
    let mut output_loopback = match spawn_loopback(&plan.output_loopback) {
        Ok(process) => process,
        Err(error) => {
            let _ = unload_modules(&ids);
            return Err(error);
        }
    };
    println!(
        "Listening for PipeWire RTP sessions on {} and mapping {} ({} channels) to output {} (sink {}, {} ms buffer, {} ms added delay). Press Ctrl-C to stop.",
        plan.rtp_sink,
        args.config.layout,
        args.config.layout.channels(),
        plan.output_sink,
        sink.name,
        args.config.latency_ms,
        args.config.output_delay_ms
    );
    let wait = wait_until_stopped();
    let loopback_cleanup = stop_loopback(&mut output_loopback);
    let module_cleanup = unload_modules(&ids);
    wait.and(loopback_cleanup).and(module_cleanup)
}

fn run_lpcm_check(args: LpcmCheckArgs) -> Result<(), String> {
    let sink = find_pipewire_sink(&args.sink)?;
    validate_receiver_sink(&sink, args.layout).map_err(|error| {
        format!(
            "{error}. Select an active output sink that exposes the requested channel positions, then recheck with `pw-dump`."
        )
    })?;
    if has_auxiliary_channel_map(&sink, args.layout) {
        let cards = pactl_output(&["list", "cards"])?;
        validate_aux_hdmi_route(&sink, &cards)?;
    }

    let test_sink = format!("q900_lpcm_test_{}", std::process::id());
    let receiver_config = ReceiverConfig {
        layout: args.layout,
        sink: sink.name.clone(),
        latency_ms: 100,
        output_delay_ms: 0,
    };
    let plan = receiver_plan(&receiver_config, &test_sink, &sink);
    let modules: Vec<ModuleSpec> = plan
        .modules
        .iter()
        .filter(|module| module.name != "module-rtp-recv")
        .cloned()
        .collect();

    if args.dry_run {
        print_lpcm_check_plan(&args, &sink, &modules, &plan.output_loopback);
        return Ok(());
    }

    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&interrupted);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|error| format!("could not install Ctrl-C handler: {error}"))?;

    let module_ids = load_modules(&modules)?;
    let mut output_loopback = match spawn_loopback(&plan.output_loopback) {
        Ok(process) => process,
        Err(error) => {
            let _ = unload_modules(&module_ids);
            return Err(error);
        }
    };

    thread::sleep(Duration::from_millis(300));
    if interrupted.load(Ordering::SeqCst) {
        let loopback_cleanup = stop_loopback(&mut output_loopback);
        let module_cleanup = unload_modules(&module_ids);
        return loopback_cleanup.and(module_cleanup);
    }
    let playback = play_lpcm_channel_test(&args, &test_sink);
    let loopback_cleanup = stop_loopback(&mut output_loopback);
    let module_cleanup = unload_modules(&module_ids);
    playback.and(loopback_cleanup).and(module_cleanup)
}

fn print_lpcm_check_plan(
    args: &LpcmCheckArgs,
    sink: &SinkCapabilities,
    modules: &[ModuleSpec],
    output_loopback: &LoopbackSpec,
) {
    println!(
        "PCM check plan: {} ({} channels) 24-bit/48 kHz test tones to {}.",
        args.layout,
        args.layout.channels(),
        sink.name
    );
    println!(
        "Each channel is played alone at low level for {} ms, followed by {} ms of silence.",
        args.tone_ms, args.gap_ms
    );
    for (index, tone) in channel_tones(args.layout).iter().enumerate() {
        println!(
            "  channel {}/{}: {} ({}, {} Hz)",
            index + 1,
            args.layout.channels(),
            tone.channel,
            channel_name(tone.channel),
            tone.frequency_hz
        );
    }
    for module in modules {
        println!("  {} {}", module.name, module.args.join(" "));
    }
    print_loopback(output_loopback);
    println!("The default sink and existing application routes are not changed.");
}

fn play_lpcm_channel_test(args: &LpcmCheckArgs, test_sink: &str) -> Result<(), String> {
    let channel_map = args
        .layout
        .pipewire_rtp_channel_map()
        .trim_matches(['[', ']'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(",");
    let channels = args.layout.channels().to_string();
    println!(
        "Playing {} ({} channels) PCM test tones to {} in this order:",
        args.layout,
        args.layout.channels(),
        args.sink
    );
    for (index, tone) in channel_tones(args.layout).iter().enumerate() {
        println!(
            "  {}/{}: {} ({}, {} Hz)",
            index + 1,
            args.layout.channels(),
            tone.channel,
            channel_name(tone.channel),
            tone.frequency_hz
        );
    }
    println!("Tones are low-level; press Ctrl-C to stop.");
    let mut player = Command::new("pw-cat")
        .args([
            "--playback",
            "--raw",
            "--target",
            test_sink,
            "--rate",
            "48000",
            "--channels",
            &channels,
            "--channel-map",
            &channel_map,
            "--format",
            "s24",
            "-",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start pw-cat: {error}"))?;
    let Some(mut input) = player.stdin.take() else {
        let _ = player.kill();
        let _ = player.wait();
        return Err("could not open pw-cat input stream".to_owned());
    };
    let result = std::io::stdout()
        .flush()
        .map_err(|error| format!("could not flush test progress: {error}"))
        .and_then(|()| {
            write_channel_identification_tones(args.layout, args.tone_ms, args.gap_ms, &mut input)
                .map_err(|error| format!("could not send channel test audio: {error}"))
        });
    drop(input);
    if result.is_err() {
        let _ = player.kill();
    }
    let status = player
        .wait()
        .map_err(|error| format!("could not wait for pw-cat: {error}"))?;
    result?;
    if !status.success() {
        return Err(format!("pw-cat exited with status {status}"));
    }
    Ok(())
}

fn channel_name(channel: &str) -> &'static str {
    match channel {
        "FL" => "front left",
        "FR" => "front right",
        "FC" => "front center",
        "LFE" => "subwoofer/LFE",
        "SL" => "side left",
        "SR" => "side right",
        "RL" => "rear left",
        "RR" => "rear right",
        _ => "unknown",
    }
}

fn find_pipewire_sink(name: &str) -> Result<SinkCapabilities, String> {
    let output = Command::new("pw-dump")
        .output()
        .map_err(|error| format!("could not run pw-dump: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "pw-dump failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let dump = String::from_utf8(output.stdout)
        .map_err(|error| format!("pw-dump returned non-UTF-8 output: {error}"))?;
    parse_pipewire_sinks(&dump)?
        .into_iter()
        .find(|sink| sink.name == name)
        .ok_or_else(|| format!("sink {name:?} is not an active PipeWire audio sink"))
}

fn print_mirror_plan(plan: &MirrorPlan, config: &MirrorConfig) {
    println!(
        "Stereo mirror plan: 2-channel s24be/48000 Hz bus to {} outputs",
        config.outputs.len()
    );
    println!("System sink: {}", plan.bus_sink);
    for module in &plan.modules {
        println!("  {} {}", module.name, module.args.join(" "));
    }
    for (index, (loopback, output)) in plan.outputs.iter().zip(&config.outputs).enumerate() {
        println!(
            "Output {}: {} ({} ms added delay)",
            index + 1,
            output.sink,
            output.delay_ms
        );
        print_loopback(loopback);
    }
}

fn print_sender_plan(plan: &q900_bridge::SenderPlan, config: &SenderConfig) {
    println!(
        "Sender plan: {} ({} channels) s24be/{} Hz RTP to {}:{}",
        config.layout,
        config.layout.channels(),
        config.rate,
        config.destination,
        config.port
    );
    if let Some(source_ip) = config.source_ip {
        println!("RTP source address: {source_ip}");
    }
    println!("System sink: {}", plan.bus_sink);
    for module in &plan.modules {
        println!("  {} {}", module.name, module.args.join(" "));
    }
    print_loopback(&plan.local_mirror);
    println!("Local mirror sink: {}", config.local_sink);
}

fn loopback_args(spec: &LoopbackSpec) -> Vec<String> {
    vec![
        "--name".to_owned(),
        spec.name.clone(),
        "--capture".to_owned(),
        spec.capture.clone(),
        "--capture-props".to_owned(),
        r#"{"stream.capture.sink":true}"#.to_owned(),
        "--playback".to_owned(),
        spec.playback.clone(),
        "--channels".to_owned(),
        spec.channels.to_string(),
        "--channel-map".to_owned(),
        spec.channel_map.clone(),
        "--latency".to_owned(),
        spec.latency_ms.to_string(),
        "--delay".to_owned(),
        format!("{:.3}", spec.delay_ms as f64 / 1000.0),
    ]
}

fn print_loopback(spec: &LoopbackSpec) {
    println!("  pw-loopback {}", loopback_args(spec).join(" "));
}

fn spawn_loopback(spec: &LoopbackSpec) -> Result<Child, String> {
    Command::new("pw-loopback")
        .args(loopback_args(spec))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start pw-loopback: {error}"))
}

fn stop_loopback(process: &mut Child) -> Result<(), String> {
    if process
        .try_wait()
        .map_err(|error| format!("could not check pw-loopback process: {error}"))?
        .is_none()
    {
        process
            .kill()
            .map_err(|error| format!("could not stop pw-loopback: {error}"))?;
    }
    process
        .wait()
        .map(|_| ())
        .map_err(|error| format!("could not reap pw-loopback process: {error}"))
}

fn load_modules(specs: &[ModuleSpec]) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for spec in specs {
        let mut args = vec!["load-module", spec.name.as_str()];
        args.extend(spec.args.iter().map(String::as_str));
        match pactl_output(&args) {
            Ok(id) => ids.push(id.trim().to_owned()),
            Err(error) => {
                let _ = unload_modules(&ids);
                return Err(format!("could not load {}: {error}", spec.name));
            }
        }
    }
    Ok(ids)
}

fn unload_modules(ids: &[String]) -> Result<(), String> {
    let mut failures = Vec::new();
    for id in ids.iter().rev() {
        if let Err(error) = pactl(&["unload-module", id]) {
            failures.push(format!("module {id}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("failed to unload {}", failures.join("; ")))
    }
}

struct AudioSession {
    module_ids: Vec<String>,
    bus_sink: String,
    old_default: String,
    prior_inputs: HashMap<String, String>,
    loopback_processes: Vec<Child>,
}

impl AudioSession {
    fn stop(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        for mut process in self.loopback_processes.drain(..).rev() {
            if let Err(error) = stop_loopback(&mut process) {
                failures.push(error);
            }
        }
        failures.extend(restore_default_sink(&self.bus_sink, &self.old_default));
        failures.extend(restore_stream_sinks(self));
        if let Err(error) = unload_modules(&self.module_ids) {
            failures.push(error);
        }
        self.module_ids.clear();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

fn restore_default_sink(bus_sink: &str, old_default: &str) -> Vec<String> {
    match pactl_output(&["get-default-sink"]) {
        Ok(current) if current.trim() == bus_sink => pactl(&["set-default-sink", old_default])
            .err()
            .map(|error| vec![format!("restore default sink: {error}")])
            .unwrap_or_default(),
        Ok(_) => Vec::new(),
        Err(error) => vec![format!("read default sink: {error}")],
    }
}

fn restore_stream_sinks(session: &AudioSession) -> Vec<String> {
    let mut failures = Vec::new();
    match current_sink_inputs() {
        Ok(inputs) => {
            for (input_id, sink_name) in inputs {
                if sink_name != session.bus_sink {
                    continue;
                }
                let target = session
                    .prior_inputs
                    .get(&input_id)
                    .unwrap_or(&session.old_default);
                if let Err(error) = pactl(&["move-sink-input", &input_id, target]) {
                    failures.push(format!("restore audio stream {input_id}: {error}"));
                }
            }
        }
        Err(error) => failures.push(format!("find audio streams: {error}")),
    }
    failures
}

fn move_inputs_to_bus(session: &AudioSession) -> Result<(), String> {
    for input_id in session.prior_inputs.keys() {
        if let Err(error) = pactl(&["move-sink-input", input_id, &session.bus_sink]) {
            // A stream can disappear between enumeration and movement.
            if !error.contains("No such entity") {
                return Err(format!("could not route audio stream {input_id}: {error}"));
            }
        }
    }
    Ok(())
}

fn current_sink_inputs() -> Result<HashMap<String, String>, String> {
    let sinks = pactl_output(&["list", "short", "sinks"])?;
    let sink_names: HashMap<String, String> = sinks
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_owned(), fields.next()?.to_owned()))
        })
        .collect();
    let inputs = pactl_output(&["list", "short", "sink-inputs"])?;
    Ok(inputs
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let id = fields.next()?.to_owned();
            let sink_id = fields.next()?;
            Some((id, sink_names.get(sink_id)?.clone()))
        })
        .collect())
}

fn pactl(args: &[&str]) -> Result<(), String> {
    pactl_output(args).map(|_| ())
}

fn pactl_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|error| format!("could not run pactl: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("pactl returned non-UTF-8 output: {error}"))
}

fn wait_until_stopped() -> Result<(), String> {
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&stopped);
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|error| format!("could not install Ctrl-C handler: {error}"))?;
    while !stopped.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn print_usage() {
    println!(
        "Usage:\n  q900-bridge mirror --output SINK_NAME[=DELAY_MS] --output SINK_NAME[=DELAY_MS]... [--latency-ms MS] [--dry-run]\n  q900-bridge calibrate --microphone SOURCE --output SINK --output SINK [--dry-run] [--json]\n  q900-bridge web [--port PORT]\n  q900-bridge sender [--layout stereo|5.1|7.1] [--destination-ip IPv4] [--source-ip IPv4] [--port PORT] [--local-sink NAME] [--latency-ms MS] [--local-delay-ms MS] [--dry-run]\n  q900-bridge receiver --sink NAME [--layout stereo|5.1|7.1] [--latency-ms MS] [--output-delay-ms MS] [--dry-run]\n  q900-bridge lpcm-check --sink NAME [--layout stereo|5.1|7.1] [--tone-ms MS] [--gap-ms MS] [--dry-run]\n  q900-bridge sync-report --input OBSERVATIONS.csv\n\nThe web command serves a local PipeWire device selector at http://127.0.0.1:8765 by default. The mirror command routes stereo system audio to any number of selected PipeWire outputs; append =DELAY_MS to set a fixed delay for one output. Bluetooth A2DP may use SBC; optical S/PDIF carries stereo PCM but requires a physical optical connection. The sender mirrors audio to one local sink and sends raw 24-bit/48 kHz RTP in stereo, 5.1, or 7.1. Set --source-ip to bind RTP to a specific local interface address; otherwise the system routing table selects the interface. Use lpcm-check to play one channel at a time through a selected output sink. Fixed delays do not compensate clock drift. The calibrate command measures acoustic arrival times and proposes per-output fixed delays; it does not change delays automatically or correct clock drift."
    );
}

#[cfg(test)]
mod web_command_tests {
    use super::parse_web_port;

    #[test]
    fn web_port_defaults_and_accepts_both_u16_boundaries() {
        assert_eq!(parse_web_port(&[]).unwrap(), 8765);
        assert_eq!(
            parse_web_port(&["--port".to_owned(), "1".to_owned()]).unwrap(),
            1
        );
        assert_eq!(
            parse_web_port(&["--port".to_owned(), "65535".to_owned()]).unwrap(),
            u16::MAX
        );
    }

    #[test]
    fn web_port_rejects_zero_overflow_and_non_numeric_values() {
        for value in ["0", "65536", "invalid"] {
            let args = ["--port".to_owned(), value.to_owned()];
            assert!(parse_web_port(&args).is_err());
        }
    }
}

#[cfg(test)]
mod calibration_argument_tests {
    use super::parse_calibration_args;

    fn valid_args(extra: &[&str]) -> Vec<String> {
        [
            "--microphone",
            "brio-source",
            "--output",
            "hdmi-sink",
            "--output",
            "q900t-sink",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn calibration_json_mode_is_opt_in_and_dry_run_is_explicit() {
        let defaults = parse_calibration_args(&valid_args(&[])).unwrap();
        assert!(!defaults.dry_run);
        assert!(!defaults.json);

        let dry_run = parse_calibration_args(&valid_args(&["--dry-run"])).unwrap();
        assert!(dry_run.dry_run);
        assert!(!dry_run.json);

        let json = parse_calibration_args(&valid_args(&["--json"])).unwrap();
        assert!(!json.dry_run);
        assert!(json.json);
    }

    #[test]
    fn calibration_json_output_cannot_be_combined_with_dry_run() {
        let error = match parse_calibration_args(&valid_args(&["--dry-run", "--json"])) {
            Ok(_) => panic!("dry run cannot request JSON report output"),
            Err(error) => error,
        };

        assert!(error.contains("--dry-run and --json cannot be combined"));
    }
}
