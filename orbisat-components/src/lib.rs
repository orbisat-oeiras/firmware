#![no_std]

use orbisat::comms::{ByteSink, ByteSource};

#[derive(Debug)]
pub struct ConsoleByteSink;

impl ByteSink for ConsoleByteSink {
    async fn sink(&mut self, buf: &[u8]) {
        defmt::info!("Outbound packet: {:02X}", buf);
    }
}

#[derive(Debug)]
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
        defmt::info!("Uart sending");
        // TODO: proper error handling
        self.0.write_all(buf).await.unwrap();
        self.0.flush().await.unwrap();
    }
}

#[derive(Debug)]
pub struct SerialByteSource<R>(R)
where
    R: embedded_io_async::Read;

impl<R> SerialByteSource<R>
where
    R: embedded_io_async::Read,
{
    pub fn new(read: R) -> Self {
        Self(read)
    }
}

impl<R> ByteSource for SerialByteSource<R>
where
    R: embedded_io_async::Read,
{
    async fn fill(&mut self, buf: &mut [u8]) -> usize {
        // TODO: error handling
        self.0.read(buf).await.unwrap()
    }
}
