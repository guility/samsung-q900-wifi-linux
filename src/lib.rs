use serde_json::Value;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::str::FromStr;

pub mod lpcm;
pub mod sync;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLayout {
    Stereo,
    Surround51,
    Surround71,
}

struct ChannelMaps {
    rtp: &'static str,
    output: &'static str,
    auxiliary: &'static str,
}

const SURROUND_51_CHANNEL_MAPS: ChannelMaps = ChannelMaps {
    rtp: "front-left,front-right,front-center,lfe,rear-left,rear-right",
    output: "front-left,front-right,front-center,lfe,rear-left,rear-right",
    auxiliary: "aux0,aux1,aux2,aux3,aux4,aux5",
};

const STEREO_CHANNEL_MAPS: ChannelMaps = ChannelMaps {
    rtp: "front-left,front-right",
    output: "front-left,front-right",
    auxiliary: "aux0,aux1",
};

const SURROUND_71_CHANNEL_MAPS: ChannelMaps = ChannelMaps {
    rtp: "front-left,front-right,front-center,lfe,side-left,side-right,rear-left,rear-right",
    output: "front-left,front-right,front-center,lfe,rear-left,rear-right,side-left,side-right",
    auxiliary: "aux0,aux1,aux2,aux3,aux4,aux5,aux6,aux7",
};

impl AudioLayout {
    pub fn channels(self) -> usize {
        match self {
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    /// Channel order used by the sender bus and the RTP payload.
    pub fn rtp_channel_map(self) -> &'static str {
        self.channel_maps().rtp
    }

    pub fn output_channel_map(self) -> &'static str {
        self.channel_maps().output
    }

    pub fn auxiliary_channel_map(self) -> &'static str {
        self.channel_maps().auxiliary
    }

    fn channel_maps(self) -> &'static ChannelMaps {
        match self {
            Self::Stereo => &STEREO_CHANNEL_MAPS,
            Self::Surround51 => &SURROUND_51_CHANNEL_MAPS,
            Self::Surround71 => &SURROUND_71_CHANNEL_MAPS,
        }
    }

    pub fn pipewire_positions(self) -> &'static [&'static str] {
        match self {
            Self::Stereo => &["FL", "FR"],
            Self::Surround51 => &["FL", "FR", "FC", "LFE", "RL", "RR"],
            Self::Surround71 => &["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"],
        }
    }

    pub fn pipewire_rtp_channel_map(self) -> &'static str {
        match self {
            Self::Stereo => "[ FL FR ]",
            Self::Surround51 => "[ FL FR FC LFE RL RR ]",
            Self::Surround71 => "[ FL FR FC LFE SL SR RL RR ]",
        }
    }
}

impl FromStr for AudioLayout {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "stereo" => Ok(Self::Stereo),
            "5.1" => Ok(Self::Surround51),
            "7.1" => Ok(Self::Surround71),
            _ => Err(format!(
                "unsupported layout {value:?}; choose stereo, 5.1, or 7.1"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSpec {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderConfig {
    pub layout: AudioLayout,
    pub destination: Ipv4Addr,
    pub source_ip: Option<Ipv4Addr>,
    pub port: u16,
    pub rate: u32,
    pub local_sink: String,
    pub latency_ms: u16,
    pub local_delay_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiverConfig {
    pub layout: AudioLayout,
    pub sink: String,
    pub latency_ms: u16,
    pub output_delay_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopbackSpec {
    pub name: String,
    pub capture: String,
    pub playback: String,
    pub channels: usize,
    pub channel_map: String,
    pub latency_ms: u16,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderPlan {
    pub bus_sink: String,
    pub modules: Vec<ModuleSpec>,
    pub local_mirror: LoopbackSpec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRoute {
    pub sink: String,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirrorConfig {
    pub outputs: Vec<OutputRoute>,
    pub latency_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MirrorPlan {
    pub bus_sink: String,
    pub modules: Vec<ModuleSpec>,
    pub outputs: Vec<LoopbackSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiverPlan {
    pub rtp_sink: String,
    pub output_sink: String,
    pub modules: Vec<ModuleSpec>,
    pub output_loopback: LoopbackSpec,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SinkCapabilities {
    pub name: String,
    pub channels: usize,
    pub positions: Vec<String>,
    pub device_api: Option<String>,
    pub device_name: Option<String>,
    pub profile_device: Option<usize>,
}

pub fn sender_plan(bus_sink: &str, config: &SenderConfig) -> SenderPlan {
    let channels = config.layout.channels().to_string();
    let null_sink = ModuleSpec {
        name: "module-null-sink".to_owned(),
        args: vec![
            format!("sink_name={bus_sink}"),
            "format=s24be".to_owned(),
            format!("channels={channels}"),
            format!("rate={}", config.rate),
            format!("channel_map={}", config.layout.rtp_channel_map()),
            "sink_properties=device.description=Q900_Bridge_RTP_Source".to_owned(),
        ],
    };
    let mut rtp_sender_args = vec![
        format!("source={bus_sink}.monitor"),
        "format=s24be".to_owned(),
        format!("channels={channels}"),
        format!("rate={}", config.rate),
        format!("destination_ip={}", config.destination),
        format!("port={}", config.port),
        "mtu=1200".to_owned(),
        "loop=1".to_owned(),
        "stream_name=Q900_Bridge".to_owned(),
    ];
    if let Some(source_ip) = config.source_ip {
        rtp_sender_args.push(format!("source_ip={source_ip}"));
    }
    let rtp_sender = ModuleSpec {
        name: "module-rtp-send".to_owned(),
        args: rtp_sender_args,
    };
    SenderPlan {
        bus_sink: bus_sink.to_owned(),
        modules: vec![null_sink, rtp_sender],
        local_mirror: LoopbackSpec {
            name: format!("q900_bridge_local_{}", std::process::id()),
            capture: bus_sink.to_owned(),
            playback: config.local_sink.clone(),
            channels: config.layout.channels(),
            channel_map: config.layout.pipewire_rtp_channel_map().to_owned(),
            latency_ms: config.latency_ms,
            delay_ms: config.local_delay_ms,
        },
    }
}

pub fn mirror_plan(bus_sink: &str, config: &MirrorConfig) -> MirrorPlan {
    let stereo_bus = ModuleSpec {
        name: "module-null-sink".to_owned(),
        args: vec![
            format!("sink_name={bus_sink}"),
            "format=s24be".to_owned(),
            "channels=2".to_owned(),
            "rate=48000".to_owned(),
            "channel_map=front-left,front-right".to_owned(),
            "sink_properties=device.description=Audio_Mirror_Stereo".to_owned(),
        ],
    };
    MirrorPlan {
        bus_sink: bus_sink.to_owned(),
        modules: vec![stereo_bus],
        outputs: config
            .outputs
            .iter()
            .enumerate()
            .map(|(index, output)| LoopbackSpec {
                name: format!("audio_mirror_output_{index}_{}", std::process::id()),
                capture: bus_sink.to_owned(),
                playback: output.sink.clone(),
                channels: 2,
                channel_map: "[ FL FR ]".to_owned(),
                latency_ms: config.latency_ms,
                delay_ms: output.delay_ms,
            })
            .collect(),
    }
}

pub fn receiver_plan(
    config: &ReceiverConfig,
    rtp_sink: &str,
    output: &SinkCapabilities,
) -> ReceiverPlan {
    let channels = config.layout.channels().to_string();
    let rtp_channel_map = config.layout.rtp_channel_map();
    let auxiliary_output = has_auxiliary_channel_map(output, config.layout);
    let output_sink = if auxiliary_output {
        format!("{rtp_sink}_output")
    } else {
        config.sink.clone()
    };
    let receive_sink = ModuleSpec {
        name: "module-null-sink".to_owned(),
        args: vec![
            format!("sink_name={rtp_sink}"),
            "format=s24be".to_owned(),
            format!("channels={channels}"),
            "rate=48000".to_owned(),
            format!("channel_map={rtp_channel_map}"),
            "sink_properties=device.description=Q900_Bridge_RTP_Receiver".to_owned(),
        ],
    };
    let mut modules = vec![receive_sink];
    if auxiliary_output {
        modules.push(ModuleSpec {
            name: "module-remap-sink".to_owned(),
            args: vec![
                format!("sink_name={output_sink}"),
                format!("master={}", config.sink),
                format!(
                    "master_channel_map={}",
                    config.layout.auxiliary_channel_map()
                ),
                format!("channels={channels}"),
                format!("channel_map={}", config.layout.output_channel_map()),
                "remix=1".to_owned(),
                "sink_properties=device.description=Q900_Bridge_HDMI_Channel_Map".to_owned(),
            ],
        });
    }
    let rtp_receiver = ModuleSpec {
        name: "module-rtp-recv".to_owned(),
        args: vec![
            format!("sink={rtp_sink}"),
            "sap_address=224.0.0.56".to_owned(),
            format!("latency_msec={}", config.latency_ms),
        ],
    };
    modules.push(rtp_receiver);
    ReceiverPlan {
        rtp_sink: rtp_sink.to_owned(),
        output_sink: output_sink.clone(),
        modules,
        output_loopback: LoopbackSpec {
            name: format!("q900_bridge_output_{}", std::process::id()),
            capture: rtp_sink.to_owned(),
            playback: output_sink.clone(),
            channels: config.layout.channels(),
            channel_map: config.layout.pipewire_rtp_channel_map().to_owned(),
            latency_ms: config.latency_ms,
            delay_ms: config.output_delay_ms,
        },
    }
}

pub fn validate_receiver_sink(sink: &SinkCapabilities, layout: AudioLayout) -> Result<(), String> {
    let expected = layout.pipewire_positions();
    if sink.channels < expected.len() {
        return Err(format!(
            "sink {} exposes {} channel(s); {} requires at least {}",
            sink.name,
            sink.channels,
            layout,
            expected.len()
        ));
    }
    if !matches_positions(sink, expected) && !has_auxiliary_channel_map(sink, layout) {
        return Err(format!(
            "sink {} does not expose the expected {} channel positions in order ({}) or a Pro Audio AUX channel map",
            sink.name,
            layout,
            expected.join(", ")
        ));
    }
    Ok(())
}

fn matches_positions(sink: &SinkCapabilities, expected: &[&str]) -> bool {
    sink.positions.len() >= expected.len()
        && sink.positions[..expected.len()]
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
}

pub fn has_auxiliary_channel_map(sink: &SinkCapabilities, layout: AudioLayout) -> bool {
    let channels = layout.channels();
    channels > 2
        && sink.positions.len() >= channels
        && sink.positions[..channels]
            .iter()
            .enumerate()
            .all(|(index, position)| position == &format!("AUX{index}"))
}

pub fn validate_aux_hdmi_route(sink: &SinkCapabilities, cards_dump: &str) -> Result<(), String> {
    if sink.device_api.as_deref() != Some("alsa") {
        return Ok(());
    }
    let card_name = sink.device_name.as_deref().ok_or_else(|| {
        format!(
            "could not identify the ALSA card for Pro Audio sink {}",
            sink.name
        )
    })?;
    let port_index = sink.profile_device.ok_or_else(|| {
        format!(
            "could not identify the HDMI port for Pro Audio sink {}",
            sink.name
        )
    })?;
    match card_hdmi_port_available(cards_dump, card_name, port_index) {
        Some(true) => Ok(()),
        Some(false) => Err(format!(
            "HDMI port {port_index} for {} is not reported connected; refusing multichannel playback",
            sink.name
        )),
        None => Err(format!(
            "could not confirm a connected HDMI port {port_index} for {}; connect an HDMI LPCM device and select its Pro Audio sink",
            sink.name
        )),
    }
}

fn card_hdmi_port_available(cards_dump: &str, card_name: &str, port_index: usize) -> Option<bool> {
    let card_header = format!("Name: {card_name}");
    let port_header = format!("hdmi-output-{port_index}:");
    let (_, cards) = cards_dump.split_once("Card #")?;
    for card in cards.split("Card #") {
        if !card.lines().any(|line| line.trim() == card_header) {
            continue;
        }
        let ports = card.split_once("Ports:")?.1;
        let port = ports
            .lines()
            .find(|line| line.trim_start().starts_with(&port_header))?;
        let port = port.trim();
        return Some(port.contains(", available") && !port.contains("not available"));
    }
    None
}

pub fn parse_pipewire_sinks(dump: &str) -> Result<Vec<SinkCapabilities>, String> {
    let root: Value = serde_json::from_str(dump)
        .map_err(|error| format!("could not parse PipeWire JSON: {error}"))?;
    let entries = root
        .as_array()
        .ok_or_else(|| "PipeWire JSON root is not an array".to_owned())?;
    let device_names = parse_pipewire_device_names(entries);
    Ok(entries
        .iter()
        .filter_map(|entry| parse_pipewire_sink(entry, &device_names))
        .collect())
}

fn parse_pipewire_device_names(entries: &[Value]) -> HashMap<u64, String> {
    entries
        .iter()
        .filter(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Device")
        })
        .filter_map(|entry| {
            let id = entry.get("id")?.as_u64()?;
            let name = entry
                .get("info")?
                .get("props")?
                .get("device.name")?
                .as_str()?;
            Some((id, name.to_owned()))
        })
        .collect()
}

fn parse_pipewire_sink(
    entry: &Value,
    device_names: &HashMap<u64, String>,
) -> Option<SinkCapabilities> {
    if entry.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Node") {
        return None;
    }
    let props = entry.get("info")?.get("props")?;
    let params = entry.get("info")?.get("params");
    let raw_audio_format = params
        .and_then(|params| params.get("EnumFormat"))
        .and_then(Value::as_array)
        .and_then(|formats| {
            formats.iter().find(|format| {
                format.get("mediaType").and_then(Value::as_str) == Some("audio")
                    && format.get("mediaSubtype").and_then(Value::as_str) == Some("raw")
            })
        });
    if props.get("media.class").and_then(Value::as_str) != Some("Audio/Sink") {
        return None;
    }
    let name = props.get("node.name")?.as_str()?;
    let channels = props
        .get("audio.channels")
        .and_then(Value::as_u64)
        .or_else(|| raw_audio_format.and_then(|format| format.get("channels")?.as_u64()))
        .unwrap_or_default() as usize;
    let positions = props
        .get("audio.position")
        .and_then(parse_positions)
        .or_else(|| {
            raw_audio_format.and_then(|format| format.get("position").and_then(parse_positions))
        })
        .unwrap_or_default();
    let device_api = props
        .get("device.api")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let device_name = props
        .get("device.id")
        .and_then(Value::as_u64)
        .and_then(|id| device_names.get(&id).cloned());
    let profile_device = props
        .get("card.profile.device")
        .and_then(Value::as_u64)
        .map(|device| device as usize);
    Some(SinkCapabilities {
        name: name.to_owned(),
        channels,
        positions,
        device_api,
        device_name,
        profile_device,
    })
}

fn parse_positions(value: &Value) -> Option<Vec<String>> {
    if let Some(positions) = value.as_array() {
        return Some(
            positions
                .iter()
                .filter_map(Value::as_str)
                .map(|position| position.trim().to_owned())
                .collect(),
        );
    }
    let positions = value.as_str()?.trim();
    let positions = positions
        .strip_prefix('[')
        .and_then(|positions| positions.strip_suffix(']'))
        .unwrap_or(positions);
    Some(
        positions
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|position| !position.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

impl std::fmt::Display for AudioLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stereo => "stereo",
            Self::Surround51 => "5.1",
            Self::Surround71 => "7.1",
        })
    }
}
