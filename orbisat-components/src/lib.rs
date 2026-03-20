#![no_std]

use orbisat::comms::ByteSink;

pub struct ConsoleByteSink;

impl ByteSink for ConsoleByteSink {
    async fn sink(&mut self, buf: &[u8]) {
        defmt::info!("Packet received: {}", buf);
    }
}
