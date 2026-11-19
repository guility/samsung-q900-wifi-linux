use q900_bridge::sync::{OutputPath, analyze_sync, parse_sync_csv};
use q900_bridge::{
    AudioLayout, ModuleSpec, ReceiverConfig, SenderConfig, SinkCapabilities,
    has_auxiliary_channel_map, parse_pipewire_sinks, receiver_plan, sender_plan,
    validate_aux_hdmi_route, validate_receiver_sink,
};
use std::collections::HashMap;
use std::env;
use std::net::Ipv4Addr;
use std::process::{Command, ExitCode};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

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
        "sender" => run_sender(parse_sender_args(&args[1..])?),
        "receiver" => run_receiver(parse_receiver_args(&args[1..])?),
        "sync-report" => run_sync_report(&args[1..]),
        other => Err(format!(
            "unknown command {other:?}; expected sender, receiver, or sync-report"
        )),
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
            "Recommended compensation: delay local output by {:.3} ms",
            report.recommended_delay_ms
        ),
        Some(OutputPath::Q900T) => println!(
            "Recommended compensation: delay Q900T by {:.3} ms",
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

struct ReceiverArgs {
    config: ReceiverConfig,
    dry_run: bool,
}

fn parse_sender_args(args: &[String]) -> Result<SenderArgs, String> {
    let options = parse_options(
        args,
        &[
            "--layout",
            "--destination-ip",
            "--port",
            "--local-sink",
            "--latency-ms",
        ],
        &["--dry-run"],
    )?;
    let layout = parse_layout(options.value("--layout"))?;
    let destination = parse_destination(options.value("--destination-ip"))?;
    let port = parse_port(options.value("--port"))?;
    let local_sink = parse_local_sink(options.value("--local-sink"))?;
    let latency_ms = parse_latency_option(options.value("--latency-ms"))?;

    Ok(SenderArgs {
        config: SenderConfig {
            layout,
            destination,
            port,
            rate: 48_000,
            local_sink,
            latency_ms,
        },
        dry_run: options.enabled("--dry-run"),
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
        &["--layout", "--sink", "--latency-ms"],
        &["--dry-run"],
    )?;
    let layout = parse_layout(options.value("--layout"))?;
    let sink = options
        .value("--sink")
        .ok_or_else(|| "receiver requires --sink SINK_NAME".to_owned())?
        .to_owned();
    validate_sink_token(&sink)?;
    let latency_ms = parse_latency_option(options.value("--latency-ms"))?;

    Ok(ReceiverArgs {
        config: ReceiverConfig {
            layout,
            sink,
            latency_ms,
        },
        dry_run: options.enabled("--dry-run"),
    })
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
    let mut session = SenderSession {
        module_ids,
        bus_sink: bus.clone(),
        old_default: old_default.clone(),
        prior_inputs,
    };

    if let Err(error) = pactl(&["set-default-sink", &bus]) {
        let _ = session.stop();
        return Err(error);
    }
    if let Err(error) = move_inputs_to_bus(&session) {
        let _ = session.stop();
        return Err(error);
    }

    println!(
        "Sending {}-channel 24-bit/48 kHz PCM via RTP to {}:{}; local mirror: {}.",
        args.config.layout, args.config.destination, args.config.port, args.config.local_sink
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
            "{error}. Select an active HDMI profile that exposes the requested channels, then recheck with `pw-dump`."
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
        println!(
            "Selected sink {} currently exposes {} channels: {}.",
            sink.name,
            sink.channels,
            sink.positions.join(", ")
        );
        return Ok(());
    }

    let ids = load_modules(&plan.modules)?;
    println!(
        "Listening for PipeWire RTP sessions on {} and mapping {} channels to {} (HDMI sink {}, {} ms buffer). Press Ctrl-C to stop.",
        plan.rtp_sink, args.config.layout, plan.output_sink, sink.name, args.config.latency_ms
    );
    let wait = wait_until_stopped();
    let cleanup = unload_modules(&ids);
    wait.and(cleanup)
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

fn print_sender_plan(plan: &q900_bridge::SenderPlan, config: &SenderConfig) {
    println!(
        "Sender plan: {}-channel s24be/{} Hz RTP to {}:{}",
        config.layout, config.rate, config.destination, config.port
    );
    println!("System sink: {}", plan.bus_sink);
    for module in &plan.modules {
        println!("  {} {}", module.name, module.args.join(" "));
    }
    println!("Local mirror sink: {}", config.local_sink);
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

struct SenderSession {
    module_ids: Vec<String>,
    bus_sink: String,
    old_default: String,
    prior_inputs: HashMap<String, String>,
}

impl SenderSession {
    fn stop(&mut self) -> Result<(), String> {
        let mut failures = restore_default_sink(&self.bus_sink, &self.old_default);
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

fn restore_stream_sinks(session: &SenderSession) -> Vec<String> {
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

fn move_inputs_to_bus(session: &SenderSession) -> Result<(), String> {
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
        "Usage:\n  q900-bridge sender [--layout 5.1|7.1] [--destination-ip IPv4] [--port PORT] [--local-sink NAME] [--latency-ms MS] [--dry-run]\n  q900-bridge receiver --sink NAME [--layout 5.1|7.1] [--latency-ms MS] [--dry-run]\n  q900-bridge sync-report --input OBSERVATIONS.csv\n\nThe sender mirrors system audio to a local sink and sends raw 24-bit/48 kHz RTP. The sync report analyzes acoustic measurements from CSV."
    );
}
