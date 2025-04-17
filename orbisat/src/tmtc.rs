use orbipacket::Packet;
use rppal::uart::Uart;
use tokio::sync::mpsc::Receiver;

use crate::signal::{cancellable_loop, SmartSignal};

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
        cancellable_loop!(cancel => {
            while let Some(packet) = self.channel.recv().await {
                let written = self.uart.write(packet.encode(&mut self.buffer[..])?)?;
                println!("Wrote {} bytes", written);
            }
        })
    }
}
