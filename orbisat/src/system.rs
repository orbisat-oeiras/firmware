use core::f32;
use std::time::Duration;

use circular_buffer::CircularBuffer;
use orbipacket::{DeviceId, Packet, Payload};
use tokio::sync::broadcast::{error::RecvError, Receiver, Sender};

use crate::{cancellable, signal::SmartSignal, tmtc::TmPacketSender};

pub struct HeartbeatSender {
    packet_sender: TmPacketSender,
}

impl HeartbeatSender {
    pub fn new(send: Sender<Packet>) -> Self {
        Self {
            packet_sender: TmPacketSender::new(send, DeviceId::System),
        }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        cancellable!(cancel => {
            loop {
                let packet = Payload::from_bytes(b"HEARTBEAT")?;

                self.packet_sender.send(packet).await?;
                interval.tick().await;
            }
        })
    }
}

pub struct AltitudeMonitor {
    packet_sender: TmPacketSender,
    channel: Receiver<Packet>,
    past_alt: CircularBuffer<8, f32>,
    past_time: CircularBuffer<8, u64>,
    has_moved: bool,
}

impl AltitudeMonitor {
    const AVGDIFF_THRES: f32 = 0.5;
    const ALTITUDE_THRES: f32 = 300.;

    pub fn new(channel: Receiver<Packet>, send: Sender<Packet>) -> Self {
        Self {
            channel,
            packet_sender: TmPacketSender::new(send, DeviceId::System),
            past_alt: CircularBuffer::new(),
            past_time: CircularBuffer::new(),
            has_moved: false,
        }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        cancellable!(cancel => {
            loop {
                match self.channel.recv().await {
                    Ok(packet) => {
                        if let Packet::TmPacket(packet) = packet {
                            if let DeviceId::PressureSensor = packet.device_id() {
                                let payload = packet.payload().as_bytes();
                                let time = packet.timestamp();
                                let pressure = f32::from_le_bytes((payload[0], payload[1], payload[2], payload[3]).into());
                                let altitude = 145366.45 * (1. - f32::powf(pressure/1013.25, 0.190284));

                                // println!("At altitude {}", altitude);

                                self.past_alt.push_back(altitude);
                                self.past_time.push_back(time.get());

                                let average_speed = std::iter::zip(self.past_alt.make_contiguous(), self.past_time.make_contiguous()).collect::<Vec<_>>().windows(2).map(|w| (*w[0].0 - *w[1].0).abs() / (*w[0].1 as i128 - *w[1].1 as i128).abs() as f32 * 1e9).sum::<f32>() / (self.past_alt.len() as f32);
                                println!("Average speed: {:e}", average_speed);
                            }
                        }
                    },
                    Err(RecvError::Closed) => {break;},
                    Err(RecvError::Lagged(skipped)) => {
                        println!("WARNING: AltitudeMonitor has skipped {} packets due to broadcast channel lag.", skipped);
                    }
                }
            }
        })
    }
}
