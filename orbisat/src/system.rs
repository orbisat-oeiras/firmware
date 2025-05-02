use std::time::Duration;

use orbipacket::{DeviceId, Packet, Payload};
use tokio::sync::broadcast::Sender;

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

                println!("Send packet");
                self.packet_sender.send(packet).await?;
                interval.tick().await;
            }
        })
    }
}
