use std::time::SystemTime;

use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};
use rppal::uart::Uart;
use tokio::sync::broadcast::{error::RecvError, Receiver, Sender};

use crate::signal::{cancellable, SmartSignal};

pub struct SerialPacketSink {
    uart: Uart,
    channel: Receiver<Packet>,
    buffer: [u8; Packet::encode_buffer_size()],
}

impl SerialPacketSink {
    pub fn new(uart: Uart, channel: Receiver<Packet>) -> Self {
        Self {
            uart,
            channel,
            buffer: [0u8; Packet::encode_buffer_size()],
        }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        cancellable!(cancel => {
            loop {
                match self.channel.recv().await {
                    Ok(packet) => {self.uart.write(packet.encode(&mut self.buffer[..])?)?;},
                    Err(RecvError::Closed) => {break;},
                    Err(RecvError::Lagged(skipped)) => {
                        println!("WARNING: SerialPacketSink has skipped {} packets due to broadcast channel lag.", skipped);
                    }
                }
            }
        })
    }
}

pub struct TmPacketSender {
    channel: Sender<Packet>,
    id: DeviceId,
}

impl TmPacketSender {
    pub fn new(channel: Sender<Packet>, id: DeviceId) -> Self {
        Self { channel, id }
    }

    pub async fn send(&self, payload: Payload) -> anyhow::Result<()> {
        let timestamp = Timestamp::new(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_nanos() as u64,
        );
        let packet = TmPacket::new(self.id, timestamp, payload);
        let packet = Packet::TmPacket(packet);
        self.channel.send(packet)?;

        Ok(())
    }
}
