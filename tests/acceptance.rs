use q900_bridge::{
    AudioLayout, ModuleSpec, ReceiverConfig, SenderConfig, SinkCapabilities, parse_pipewire_sinks,
    receiver_plan, sender_plan, validate_receiver_sink,
};
use std::net::Ipv4Addr;
use std::str::FromStr;

#[test]
fn accepts_only_explicit_51_and_71_layouts() {
    assert_eq!(AudioLayout::from_str("5.1").unwrap().channels(), 6);
    assert_eq!(AudioLayout::from_str("7.1").unwrap().channels(), 8);
    assert!(AudioLayout::from_str("stereo").is_err());
    assert!(AudioLayout::from_str("5.1.2").is_err());
}

#[test]
fn sender_plan_preserves_multichannel_pcm_and_mirrors_to_host_sink() {
    let config = SenderConfig {
        layout: AudioLayout::Surround71,
        destination: Ipv4Addr::new(239, 10, 20, 30),
        port: 5004,
        rate: 48_000,
        local_sink: "alsa_output.test".to_owned(),
        latency_ms: 80,
    };

    let plan = sender_plan("q900_bridge_test", &config);
    assert_eq!(plan.modules.len(), 3);
    assert_eq!(plan.modules[0].name, "module-null-sink");
    assert!(plan.modules[0].args.iter().any(|arg| arg == "channels=8"));
    assert!(
        plan.modules[0]
            .args
            .iter()
            .any(|arg| arg.contains("rear-left,rear-right,side-left,side-right"))
    );
    assert!(plan.modules[1].args.iter().any(|arg| arg == "format=s24be"));
    assert!(plan.modules[1].args.iter().any(|arg| arg == "channels=8"));
    assert!(
        plan.modules[1]
            .args
            .iter()
            .any(|arg| arg == "destination_ip=239.10.20.30")
    );
    assert!(plan.modules[1].args.iter().any(|arg| arg == "loop=1"));
    assert!(
        plan.modules[2]
            .args
            .iter()
            .any(|arg| arg == "sink=alsa_output.test")
    );
}

#[test]
fn receiver_plan_selects_pcm_format_layout_and_latency() {
    let config = ReceiverConfig {
        layout: AudioLayout::Surround51,
        sink: "alsa_output.hdmi".to_owned(),
        latency_ms: 60,
    };

    let plan = receiver_plan(&config);
    assert_eq!(plan.name, "module-rtp-recv");
    assert!(plan.args.iter().any(|arg| arg == "sink=alsa_output.hdmi"));
    assert!(plan.args.iter().any(|arg| arg == "latency_msec=60"));
}

#[test]
fn receiver_rejects_stereo_sink_for_multichannel_layout() {
    let stereo = SinkCapabilities {
        name: "alsa_output.stereo".to_owned(),
        channels: 2,
        positions: vec!["FL".to_owned(), "FR".to_owned()],
    };

    assert!(validate_receiver_sink(&stereo, AudioLayout::Surround51).is_err());
    assert!(validate_receiver_sink(&stereo, AudioLayout::Surround71).is_err());
}

#[test]
fn receiver_accepts_71_sink_for_51_and_71() {
    let surround71 = SinkCapabilities {
        name: "alsa_output.hdmi".to_owned(),
        channels: 8,
        positions: vec!["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };

    assert!(validate_receiver_sink(&surround71, AudioLayout::Surround51).is_ok());
    assert!(validate_receiver_sink(&surround71, AudioLayout::Surround71).is_ok());
}

#[test]
fn receiver_rejects_missing_channel_positions_even_if_count_is_high_enough() {
    let nonstandard = SinkCapabilities {
        name: "alsa_output.bad_map".to_owned(),
        channels: 8,
        positions: vec!["FL", "FR", "FC", "LFE", "RL", "RR", "AUX0", "AUX1"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };

    assert!(validate_receiver_sink(&nonstandard, AudioLayout::Surround71).is_err());
}

#[test]
fn parses_pipewire_dump_channel_positions_as_reported_by_pw_dump() {
    let dump = r#"[
        {
            "id": 34,
            "type": "PipeWire:Interface:Node",
            "info": {"props": {
                "media.class": "Audio/Sink",
                "node.name": "alsa_output.hdmi_surround",
                "audio.channels": 6,
                "audio.position": "[ FL, FR, FC, LFE, RL, RR ]"
            }}
        }
    ]"#;

    let sinks = parse_pipewire_sinks(dump).unwrap();
    assert_eq!(sinks.len(), 1);
    assert_eq!(sinks[0].positions, ["FL", "FR", "FC", "LFE", "RL", "RR"]);
    assert!(validate_receiver_sink(&sinks[0], AudioLayout::Surround51).is_ok());
}

#[test]
fn keeps_active_sinks_without_channel_metadata_for_explicit_rejection() {
    let dump = r#"[
        {
            "type": "PipeWire:Interface:Node",
            "info": {"props": {
                "media.class": "Audio/Sink",
                "node.name": "bluez_output.q900"
            }}
        }
    ]"#;

    let sinks = parse_pipewire_sinks(dump).unwrap();
    assert_eq!(sinks.len(), 1);
    assert_eq!(sinks[0].name, "bluez_output.q900");
    assert!(validate_receiver_sink(&sinks[0], AudioLayout::Surround51).is_err());
}

#[test]
fn module_specs_are_arguments_not_shell_text() {
    let spec = ModuleSpec {
        name: "module-loopback".to_owned(),
        args: vec!["sink=valid_sink".to_owned()],
    };
    assert_eq!(spec.name, "module-loopback");
    assert_eq!(spec.args, ["sink=valid_sink"]);
}
