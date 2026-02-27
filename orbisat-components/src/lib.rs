#![no_std]

use orbisat::PacketSink;

pub struct ConsolePacketSink;

impl PacketSink for ConsolePacketSink {
    async fn sink_bytes(&self, buf: &[u8]) {
        defmt::info!("Packet received: {}", buf);
    }
}
