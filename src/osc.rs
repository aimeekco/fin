use std::error::Error;
use std::fmt;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rosc::{OscMessage, OscPacket, OscType, decoder, encoder};

use crate::model::{Program, ScheduledEvent};

#[derive(Debug)]
pub struct OscClient {
    socket: UdpSocket,
    target: SocketAddr,
    next_node_id: AtomicI32,
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
        socket
            .set_read_timeout(Some(Duration::from_millis(750)))
            .map_err(|error| OscError::new(format!("failed to configure UDP socket: {error}")))?;

        Ok(Self {
            socket,
            target,
            next_node_id: AtomicI32::new(initial_node_id_seed()),
        })
    }

    pub fn play_bar(&self, program: &Program, events: &[ScheduledEvent]) -> Result<(), OscError> {
        self.verify_server()?;

        let mut current_offset = Duration::ZERO;
        let mut pending_releases: Vec<PendingRelease> = Vec::new();
        let bar_duration = beat_to_duration(4.0, program.effective_bpm());

        for event in events {
            let target_offset = beat_to_duration(event.beat_pos, program.effective_bpm());
            flush_releases_due(
                self,
                &mut pending_releases,
                target_offset,
                &mut current_offset,
            )?;
            sleep_until(target_offset, &mut current_offset);

            let node_id = self.next_node_id();
            let voice = voice_for_layer(&event.layer.0);
            let packet = build_trigger_packet(node_id, voice);
            self.send(&packet)?;
            pending_releases.push(PendingRelease {
                at: target_offset + voice.release_after,
                node_id,
            });
        }

        flush_releases_due(
            self,
            &mut pending_releases,
            Duration::MAX,
            &mut current_offset,
        )?;
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

    fn verify_server(&self) -> Result<(), OscError> {
        self.send(&status_packet())?;

        let mut buffer = [0u8; 2048];
        loop {
            let (size, _) = self.socket.recv_from(&mut buffer).map_err(|error| {
                OscError::new(format!(
                    "did not receive `/status.reply` from SuperCollider at {}: {error}",
                    self.target
                ))
            })?;

            let packet = decoder::decode_udp(&buffer[..size])
                .map_err(|error| OscError::new(format!("failed to decode OSC reply: {error}")))?
                .1;

            if packet_has_address(&packet, "/status.reply") {
                return Ok(());
            }
        }
    }

    fn next_node_id(&self) -> i32 {
        self.next_node_id.fetch_add(1, Ordering::Relaxed)
    }
}

fn beat_to_duration(beat_pos: f32, bpm: f32) -> Duration {
    let seconds = beat_pos as f64 * 60.0 / bpm as f64;
    Duration::from_secs_f64(seconds)
}

fn flush_releases_due(
    client: &OscClient,
    pending_releases: &mut Vec<PendingRelease>,
    deadline: Duration,
    current_offset: &mut Duration,
) -> Result<(), OscError> {
    pending_releases.sort_by_key(|release| release.at);

    while pending_releases
        .first()
        .is_some_and(|release| release.at <= deadline)
    {
        let release = pending_releases.remove(0);
        sleep_until(release.at, current_offset);
        client.send(&build_release_packet(release.node_id))?;
    }

    Ok(())
}

fn sleep_until(target_offset: Duration, current_offset: &mut Duration) {
    let wait = target_offset.saturating_sub(*current_offset);
    if !wait.is_zero() {
        thread::sleep(wait);
    }
    *current_offset = target_offset;
}

fn status_packet() -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: "/status".to_string(),
        args: Vec::new(),
    })
}

pub fn build_trigger_packet(node_id: i32, voice: VoiceConfig) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: "/s_new".to_string(),
        args: vec![
            OscType::String(voice.synth_name.to_string()),
            OscType::Int(node_id),
            OscType::Int(0),
            OscType::Int(1),
            OscType::String("freq".to_string()),
            OscType::Float(voice.freq),
            OscType::String("amp".to_string()),
            OscType::Float(voice.amp),
            OscType::String("pan".to_string()),
            OscType::Float(voice.pan),
        ],
    })
}

pub fn build_release_packet(node_id: i32) -> OscPacket {
    OscPacket::Message(OscMessage {
        addr: "/n_set".to_string(),
        args: vec![
            OscType::Int(node_id),
            OscType::String("gate".to_string()),
            OscType::Int(0),
        ],
    })
}

fn packet_has_address(packet: &OscPacket, expected: &str) -> bool {
    match packet {
        OscPacket::Message(message) => message.addr == expected,
        OscPacket::Bundle(bundle) => bundle
            .content
            .iter()
            .any(|packet| packet_has_address(packet, expected)),
    }
}

fn initial_node_id_seed() -> i32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let seed = (nanos ^ (pid << 20)) % 1_000_000_000;
    seed as i32 + 10_000
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceConfig {
    synth_name: &'static str,
    freq: f32,
    amp: f32,
    pan: f32,
    release_after: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRelease {
    at: Duration,
    node_id: i32,
}

fn voice_for_layer(name: &str) -> VoiceConfig {
    match name {
        "bd" => VoiceConfig {
            synth_name: "fin_bd",
            freq: 55.0,
            amp: 0.9,
            pan: 0.0,
            release_after: Duration::from_millis(180),
        },
        "sd" => VoiceConfig {
            synth_name: "fin_sd",
            freq: 180.0,
            amp: 0.55,
            pan: -0.1,
            release_after: Duration::from_millis(140),
        },
        "hh" => VoiceConfig {
            synth_name: "fin_hh",
            freq: 880.0,
            amp: 0.25,
            pan: 0.2,
            release_after: Duration::from_millis(70),
        },
        _ => {
            let hash = name.bytes().fold(0u32, |acc, byte| {
                acc.wrapping_mul(31).wrapping_add(byte as u32)
            });
            VoiceConfig {
                synth_name: "fin_tone",
                freq: 110.0 + (hash % 440) as f32,
                amp: 0.4,
                pan: ((hash % 21) as f32 / 10.0) - 1.0,
                release_after: Duration::from_millis(160),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_status_packet() {
        let OscPacket::Message(message) = status_packet() else {
            panic!("expected message packet");
        };

        assert_eq!(message.addr, "/status");
        assert!(message.args.is_empty());
    }

    #[test]
    fn builds_supercollider_trigger_message() {
        let voice = voice_for_layer("bd");
        let packet = build_trigger_packet(1001, voice);

        let OscPacket::Message(message) = packet else {
            panic!("expected message packet");
        };

        assert_eq!(message.addr, "/s_new");
        assert_eq!(
            message.args,
            vec![
                OscType::String("fin_bd".to_string()),
                OscType::Int(1001),
                OscType::Int(0),
                OscType::Int(1),
                OscType::String("freq".to_string()),
                OscType::Float(55.0),
                OscType::String("amp".to_string()),
                OscType::Float(0.9),
                OscType::String("pan".to_string()),
                OscType::Float(0.0),
            ]
        );
    }

    #[test]
    fn builds_release_message() {
        let packet = build_release_packet(1001);

        let OscPacket::Message(message) = packet else {
            panic!("expected message packet");
        };

        assert_eq!(message.addr, "/n_set");
        assert_eq!(
            message.args,
            vec![
                OscType::Int(1001),
                OscType::String("gate".to_string()),
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
    fn node_id_seed_starts_in_positive_application_range() {
        let seed = initial_node_id_seed();
        assert!(seed >= 10_000);
    }

    #[test]
    fn unknown_layers_fall_back_to_fin_tone() {
        let voice = voice_for_layer("bass");
        assert_eq!(voice.synth_name, "fin_tone");
    }
}
