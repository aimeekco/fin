use std::error::Error;
use std::fmt;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType, encoder};

use crate::model::{Program, ScheduledEvent};

#[derive(Debug)]
pub struct OscClient {
    socket: UdpSocket,
    target: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OscError {
    message: String,
}

impl OscError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for OscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl Error for OscError {}

impl OscClient {
    pub fn connect(host: &str, port: u16) -> Result<Self, OscError> {
        let target: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|error| OscError::new(format!("invalid OSC target {host}:{port}: {error}")))?;
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|error| OscError::new(format!("failed to bind UDP socket: {error}")))?;

        Ok(Self { socket, target })
    }

    pub fn play_bar(&self, program: &Program, events: &[ScheduledEvent]) -> Result<(), OscError> {
        let mut current_offset = Duration::ZERO;
        let bar_duration = beat_to_duration(4.0, program.effective_bpm());

        for event in events {
            let target_offset = beat_to_duration(event.beat_pos, program.effective_bpm());
            sleep_until(target_offset, &mut current_offset);
            let voice = voice_for_layer(&event.layer.0);
            let packet = build_trigger_packet(&event.layer.0, voice);
            self.send(&packet)?;
        }

        sleep_until(bar_duration, &mut current_offset);

        Ok(())
    }

    fn send(&self, packet: &OscPacket) -> Result<(), OscError> {
        let bytes = encoder::encode(packet)
            .map_err(|error| OscError::new(format!("failed to encode OSC packet: {error}")))?;
        self.socket
            .send_to(&bytes, self.target)
            .map_err(|error| OscError::new(format!("failed to send OSC packet: {error}")))?;
        Ok(())
    }
}

fn beat_to_duration(beat_pos: f32, bpm: f32) -> Duration {
    let seconds = beat_pos as f64 * 60.0 / bpm as f64;
    Duration::from_secs_f64(seconds)
}

fn sleep_until(target_offset: Duration, current_offset: &mut Duration) {
    let wait = target_offset.saturating_sub(*current_offset);
    if !wait.is_zero() {
        thread::sleep(wait);
    }
    *current_offset = target_offset;
}

pub fn build_trigger_packet(sound_name: &str, voice: VoiceConfig) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: "/dirt/play".to_string(),
        args: vec![
            OscType::String("s".to_string()),
            OscType::String(sound_name.to_string()),
            OscType::String("gain".to_string()),
            OscType::Float(voice.gain),
            OscType::String("pan".to_string()),
            OscType::Float(voice.pan),
            OscType::String("freq".to_string()),
            OscType::Float(voice.freq),
            OscType::String("speed".to_string()),
            OscType::Float(voice.speed),
            OscType::String("sustain".to_string()),
            OscType::Float(voice.sustain.as_secs_f32()),
            OscType::String("orbit".to_string()),
            OscType::Int(0),
        ],
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceConfig {
    gain: f32,
    freq: f32,
    pan: f32,
    speed: f32,
    sustain: Duration,
}

fn voice_for_layer(name: &str) -> VoiceConfig {
    match name {
        "bd" => VoiceConfig {
            gain: 1.0,
            freq: 55.0,
            pan: 0.0,
            speed: 1.0,
            sustain: Duration::from_millis(180),
        },
        "sd" => VoiceConfig {
            gain: 0.9,
            freq: 180.0,
            pan: -0.1,
            speed: 1.0,
            sustain: Duration::from_millis(140),
        },
        "hh" => VoiceConfig {
            gain: 0.7,
            freq: 880.0,
            pan: 0.2,
            speed: 1.0,
            sustain: Duration::from_millis(70),
        },
        _ => VoiceConfig {
            gain: 0.8,
            freq: 440.0,
            pan: 0.0,
            speed: 1.0,
            sustain: Duration::from_millis(160),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_supercollider_trigger_message() {
        let voice = voice_for_layer("bd");
        let packet = build_trigger_packet("bd", voice);

        let OscPacket::Message(message) = packet else {
            panic!("expected message packet");
        };

        assert_eq!(message.addr, "/dirt/play");
        assert_eq!(
            message.args,
            vec![
                OscType::String("s".to_string()),
                OscType::String("bd".to_string()),
                OscType::String("gain".to_string()),
                OscType::Float(1.0),
                OscType::String("pan".to_string()),
                OscType::Float(0.0),
                OscType::String("freq".to_string()),
                OscType::Float(55.0),
                OscType::String("speed".to_string()),
                OscType::Float(1.0),
                OscType::String("sustain".to_string()),
                OscType::Float(0.18),
                OscType::String("orbit".to_string()),
                OscType::Int(0),
            ]
        );
    }

    #[test]
    fn uses_default_bpm_when_not_specified() {
        let duration = beat_to_duration(4.0, 120.0);
        assert_eq!(duration, Duration::from_secs(2));
    }

    #[test]
    fn unknown_layers_keep_generic_voice_defaults() {
        let voice = voice_for_layer("bass");
        assert_eq!(voice.gain, 0.8);
        assert_eq!(voice.freq, 440.0);
    }
}
