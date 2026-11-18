use serde_json::Value;
use std::net::Ipv4Addr;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioLayout {
    Surround51,
    Surround71,
}

impl AudioLayout {
    pub fn channels(self) -> usize {
        match self {
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    /// Channel order used by the sender bus and the RTP payload.
    pub fn rtp_channel_map(self) -> &'static str {
        match self {
            Self::Surround51 => "front-left,front-right,front-center,lfe,rear-left,rear-right",
            Self::Surround71 => {
                "front-left,front-right,front-center,lfe,side-left,side-right,rear-left,rear-right"
            }
        }
    }

    pub fn pipewire_positions(self) -> &'static [&'static str] {
        match self {
            Self::Surround51 => &["FL", "FR", "FC", "LFE", "RL", "RR"],
            Self::Surround71 => &["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"],
        }
    }
}

impl FromStr for AudioLayout {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "5.1" => Ok(Self::Surround51),
            "7.1" => Ok(Self::Surround71),
            _ => Err(format!("unsupported layout {value:?}; choose 5.1 or 7.1")),
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
    pub port: u16,
    pub rate: u32,
    pub local_sink: String,
    pub latency_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiverConfig {
    pub layout: AudioLayout,
    pub sink: String,
    pub latency_ms: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SenderPlan {
    pub bus_sink: String,
    pub modules: Vec<ModuleSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiverPlan {
    pub rtp_sink: String,
    pub modules: Vec<ModuleSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SinkCapabilities {
    pub name: String,
    pub channels: usize,
    pub positions: Vec<String>,
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
            "sink_properties=device.description=Q900_Bridge_Multichannel".to_owned(),
        ],
    };
    let rtp_sender = ModuleSpec {
        name: "module-rtp-send".to_owned(),
        args: vec![
            format!("source={bus_sink}.monitor"),
            "format=s24be".to_owned(),
            format!("channels={channels}"),
            format!("rate={}", config.rate),
            format!("destination_ip={}", config.destination),
            format!("port={}", config.port),
            "mtu=1200".to_owned(),
            "loop=1".to_owned(),
            "stream_name=Q900_Bridge".to_owned(),
        ],
    };
    let local_mirror = ModuleSpec {
        name: "module-loopback".to_owned(),
        args: vec![
            format!("source={bus_sink}.monitor"),
            format!("sink={}", config.local_sink),
            format!("latency_msec={}", config.latency_ms),
            "sink_input_properties=media.name=Q900_Bridge_Local_Mirror".to_owned(),
        ],
    };

    SenderPlan {
        bus_sink: bus_sink.to_owned(),
        modules: vec![null_sink, rtp_sender, local_mirror],
    }
}

pub fn receiver_plan(config: &ReceiverConfig, rtp_sink: &str) -> ReceiverPlan {
    let channels = config.layout.channels().to_string();
    let channel_map = config.layout.rtp_channel_map();
    let receive_sink = ModuleSpec {
        name: "module-null-sink".to_owned(),
        args: vec![
            format!("sink_name={rtp_sink}"),
            "format=s24be".to_owned(),
            format!("channels={channels}"),
            "rate=48000".to_owned(),
            format!("channel_map={channel_map}"),
            "sink_properties=device.description=Q900_Bridge_RTP_Receiver".to_owned(),
        ],
    };
    let rtp_receiver = ModuleSpec {
        name: "module-rtp-recv".to_owned(),
        args: vec![
            format!("sink={rtp_sink}"),
            "sap_address=224.0.0.56".to_owned(),
            format!("latency_msec={}", config.latency_ms),
        ],
    };
    let channel_mapper = ModuleSpec {
        name: "module-loopback".to_owned(),
        args: vec![
            format!("source={rtp_sink}.monitor"),
            format!("sink={}", config.sink),
            format!("latency_msec={}", config.latency_ms),
            format!("channels={channels}"),
            format!("channel_map={channel_map}"),
            "remix=1".to_owned(),
            "source_dont_move=1".to_owned(),
            "sink_dont_move=1".to_owned(),
            "sink_input_properties=media.name=Q900_Bridge_RTP_Output".to_owned(),
        ],
    };

    ReceiverPlan {
        rtp_sink: rtp_sink.to_owned(),
        modules: vec![receive_sink, rtp_receiver, channel_mapper],
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
    if sink.positions.len() < expected.len()
        || sink.positions[..expected.len()]
            .iter()
            .map(String::as_str)
            .ne(expected.iter().copied())
    {
        return Err(format!(
            "sink {} does not expose the expected {} channel positions in order: {}",
            sink.name,
            layout,
            expected.join(", ")
        ));
    }
    Ok(())
}

pub fn parse_pipewire_sinks(dump: &str) -> Result<Vec<SinkCapabilities>, String> {
    let root: Value = serde_json::from_str(dump)
        .map_err(|error| format!("could not parse PipeWire JSON: {error}"))?;
    let entries = root
        .as_array()
        .ok_or_else(|| "PipeWire JSON root is not an array".to_owned())?;
    let mut sinks = Vec::new();

    for entry in entries {
        if entry.get("type").and_then(Value::as_str) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let Some(props) = entry.get("info").and_then(|info| info.get("props")) else {
            continue;
        };
        if props.get("media.class").and_then(Value::as_str) != Some("Audio/Sink") {
            continue;
        }
        let Some(name) = props.get("node.name").and_then(Value::as_str) else {
            continue;
        };
        let channels = props
            .get("audio.channels")
            .and_then(Value::as_u64)
            .unwrap_or_default() as usize;
        let positions = props
            .get("audio.position")
            .and_then(parse_positions)
            .unwrap_or_default();
        sinks.push(SinkCapabilities {
            name: name.to_owned(),
            channels,
            positions,
        });
    }

    Ok(sinks)
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
            Self::Surround51 => "5.1",
            Self::Surround71 => "7.1",
        })
    }
}
