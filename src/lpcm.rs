use crate::AudioLayout;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelTone {
    pub channel: &'static str,
    pub frequency_hz: u32,
}

const TONES_51: [ChannelTone; 6] = [
    ChannelTone {
        channel: "FL",
        frequency_hz: 440,
    },
    ChannelTone {
        channel: "FR",
        frequency_hz: 520,
    },
    ChannelTone {
        channel: "FC",
        frequency_hz: 600,
    },
    ChannelTone {
        channel: "LFE",
        frequency_hz: 80,
    },
    ChannelTone {
        channel: "RL",
        frequency_hz: 680,
    },
    ChannelTone {
        channel: "RR",
        frequency_hz: 760,
    },
];

const TONES_STEREO: [ChannelTone; 2] = [
    ChannelTone {
        channel: "FL",
        frequency_hz: 440,
    },
    ChannelTone {
        channel: "FR",
        frequency_hz: 520,
    },
];

const TONES_71: [ChannelTone; 8] = [
    TONES_51[0],
    TONES_51[1],
    TONES_51[2],
    TONES_51[3],
    ChannelTone {
        channel: "SL",
        frequency_hz: 840,
    },
    ChannelTone {
        channel: "SR",
        frequency_hz: 920,
    },
    TONES_51[4],
    TONES_51[5],
];

pub fn channel_tones(layout: AudioLayout) -> &'static [ChannelTone] {
    match layout {
        AudioLayout::Stereo => &TONES_STEREO,
        AudioLayout::Surround51 => &TONES_51,
        AudioLayout::Surround71 => &TONES_71,
    }
}

/// Writes one low-level native-endian 24-bit tone at a time, with all other
/// channels silent. The sequence is interleaved in the RTP channel order.
pub fn write_channel_identification_tones<W: Write>(
    layout: AudioLayout,
    tone_ms: u32,
    gap_ms: u32,
    output: &mut W,
) -> io::Result<()> {
    if !(200..=3_000).contains(&tone_ms) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tone duration must be between 200 and 3000 ms",
        ));
    }
    if gap_ms > 2_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "gap duration must be between 0 and 2000 ms",
        ));
    }
    let channels = layout.channels();
    for (active_channel, tone) in channel_tones(layout).iter().enumerate() {
        write_tone(channels, active_channel, tone.frequency_hz, tone_ms, output)?;
        write_silence(channels, gap_ms, output)?;
    }
    Ok(())
}

fn write_tone<W: Write>(
    channels: usize,
    active_channel: usize,
    frequency_hz: u32,
    duration_ms: u32,
    output: &mut W,
) -> io::Result<()> {
    let frames = (48_000_u64 * u64::from(duration_ms) / 1_000) as usize;
    // The public writer enforces a 200 ms minimum, so a 5 ms fade always fits.
    let fade_frames = 48_000_usize / 200;
    let mut pcm = Vec::with_capacity(frames * channels * 3);
    for frame in 0..frames {
        let fade_in = (frame as f64 / fade_frames as f64).min(1.0);
        let fade_out = ((frames - frame) as f64 / fade_frames as f64).min(1.0);
        let envelope = fade_in.min(fade_out);
        let phase = std::f64::consts::TAU * f64::from(frequency_hz) * frame as f64 / 48_000.0;
        let sample = (phase.sin() * envelope * 0.05 * 8_388_607.0) as i32;
        for channel in 0..channels {
            if channel == active_channel {
                let bytes = sample.to_ne_bytes();
                #[cfg(target_endian = "little")]
                pcm.extend_from_slice(&bytes[..3]);
                #[cfg(target_endian = "big")]
                pcm.extend_from_slice(&bytes[1..]);
            } else {
                pcm.extend_from_slice(&[0, 0, 0]);
            }
        }
    }
    output.write_all(&pcm)
}

fn write_silence<W: Write>(channels: usize, duration_ms: u32, output: &mut W) -> io::Result<()> {
    let frames = (48_000_u64 * u64::from(duration_ms) / 1_000) as usize;
    let silence = vec![0; frames * channels * 3];
    output.write_all(&silence)
}
