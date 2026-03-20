#![no_std]

use orbisat::comms::ByteSink;

pub struct ConsoleByteSink;

impl ByteSink for ConsoleByteSink {
    async fn sink(&mut self, buf: &[u8]) {
        defmt::info!("Packet received: {}", buf);
    }
}

pub struct SerialByteSink<W>(W)
where
    W: embedded_io_async::Write;

impl<W> SerialByteSink<W>
where
    W: embedded_io_async::Write,
{
    pub fn new(write: W) -> Self {
        Self(write)
    }
}

impl<W> ByteSink for SerialByteSink<W>
where
    W: embedded_io_async::Write,
{
    async fn sink(&mut self, buf: &[u8]) {
        // TODO: proper error handling
        self.0.write_all(buf).await.unwrap();
        self.0.flush().await.unwrap();
    }
}
