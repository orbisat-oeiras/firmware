use std::time::Duration;

use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};

use crate::{cancellable_loop, signal::SmartSignal};

pub struct DummySender {
    pub(crate) send: tokio::sync::mpsc::Sender<Packet>,
}

impl DummySender {
    pub fn new(send: tokio::sync::mpsc::Sender<Packet>) -> Self {
        Self { send }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        cancellable_loop!(cancel => {
            loop {
                let packet = TmPacket::new(
                    DeviceId::MissingDevice,
                    Timestamp::new(10),
                    Payload::from_bytes(&[11])?,
                );

                println!("Send packet");
                self.send.send(Packet::TmPacket(packet)).await?;
                interval.tick().await;
            }
        })
    }
}
