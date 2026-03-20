use std::error::Error;
use std::fmt;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::Duration;

use rosc::{OscMessage, OscPacket, OscType, encoder};

use crate::model::{EventParams, Program, ScheduledEvent, SoundTarget};

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
            let voice = voice_for_layer(&event.sound.name).with_params(event.params.clone());
            let packet = build_trigger_packet(&event.sound, voice);
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

pub fn build_trigger_packet(sound: &SoundTarget, voice: VoiceConfig) -> OscPacket {
    let mut args = vec![
        OscType::String("s".to_string()),
        OscType::String(sound.name.clone()),
        OscType::String("gain".to_string()),
        OscType::Float(voice.gain),
        OscType::String("pan".to_string()),
        OscType::Float(voice.pan),
        OscType::String("speed".to_string()),
        OscType::Float(voice.speed),
        OscType::String("sustain".to_string()),
        OscType::Float(voice.sustain.as_secs_f32()),
        OscType::String("orbit".to_string()),
        OscType::Int(0),
    ];

    match voice.note {
        Some(note) => args.extend([OscType::String("note".to_string()), OscType::Float(note)]),
        None => args.extend([
            OscType::String("freq".to_string()),
            OscType::Float(voice.freq),
        ]),
    }

    if let Some(index) = sound.index {
        args.extend([OscType::String("n".to_string()), OscType::Int(index)]);
    }

    OscPacket::Message(OscMessage {
        addr: "/dirt/play".to_string(),
        args,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceConfig {
    gain: f32,
    freq: f32,
    note: Option<f32>,
    pan: f32,
    speed: f32,
    sustain: Duration,
}

impl VoiceConfig {
    fn with_params(self, params: EventParams) -> Self {
        Self {
            gain: params.gain.unwrap_or(self.gain),
            note: params.note.or(self.note),
            pan: params.pan.unwrap_or(self.pan),
            speed: params.speed.unwrap_or(self.speed),
            sustain: params
                .sustain
                .map(Duration::from_secs_f32)
                .unwrap_or(self.sustain),
            ..self
        }
    }
}

fn voice_for_layer(name: &str) -> VoiceConfig {
    match name {
        "bd" => VoiceConfig {
            gain: 1.0,
            freq: 55.0,
            note: None,
            pan: 0.0,
            speed: 1.0,
            sustain: Duration::from_millis(180),
        },
        "sd" => VoiceConfig {
            gain: 0.9,
            freq: 180.0,
            note: None,
            pan: -0.1,
            speed: 1.0,
            sustain: Duration::from_millis(140),
        },
        "hh" => VoiceConfig {
            gain: 0.7,
            freq: 880.0,
            note: None,
            pan: 0.2,
            speed: 1.0,
            sustain: Duration::from_millis(70),
        },
        _ => VoiceConfig {
            gain: 0.8,
            freq: 440.0,
            note: None,
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
        let packet = build_trigger_packet(
            &SoundTarget {
                name: "bd".to_string(),
                index: None,
            },
            voice,
        );

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
                OscType::String("speed".to_string()),
                OscType::Float(1.0),
                OscType::String("sustain".to_string()),
                OscType::Float(0.18),
                OscType::String("orbit".to_string()),
                OscType::Int(0),
                OscType::String("freq".to_string()),
                OscType::Float(55.0),
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

    #[test]
    fn applies_event_parameter_overrides() {
        let voice = voice_for_layer("bd").with_params(EventParams {
            gain: Some(0.4),
            pan: Some(-0.5),
            speed: Some(1.25),
            sustain: None,
            note: None,
            note_label: None,
        });
        assert_eq!(voice.gain, 0.4);
        assert_eq!(voice.pan, -0.5);
        assert_eq!(voice.speed, 1.25);
        assert_eq!(voice.freq, 55.0);
    }

    #[test]
    fn includes_sample_index_when_present() {
        let voice = voice_for_layer("bd");
        let packet = build_trigger_packet(
            &SoundTarget {
                name: "bd".to_string(),
                index: Some(3),
            },
            voice,
        );

        let OscPacket::Message(message) = packet else {
            panic!("expected message packet");
        };

        assert!(
            message
                .args
                .ends_with(&[OscType::String("n".to_string()), OscType::Int(3),])
        );
    }

    #[test]
    fn includes_note_when_present() {
        let voice = voice_for_layer("bass").with_params(EventParams {
            gain: None,
            pan: None,
            speed: None,
            sustain: None,
            note: Some(-5.0),
            note_label: Some("g4".to_string()),
        });
        let packet = build_trigger_packet(
            &SoundTarget {
                name: "bass".to_string(),
                index: None,
            },
            voice,
        );

        let OscPacket::Message(message) = packet else {
            panic!("expected message packet");
        };

        assert!(message.args.contains(&OscType::String("note".to_string())));
        assert!(message.args.contains(&OscType::Float(-5.0)));
    }
}
