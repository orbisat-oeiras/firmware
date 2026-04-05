#![no_std]

pub mod primary;

use embassy_time::Instant;
use orbipacket::{DeviceId, Payload};
use orbisat::{
    Component,
    comms::{ByteSink, ByteSource, CommunicationError},
};

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

#[derive(thiserror::Error, Debug)]
pub enum TimeSyncError {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error("time sync telecommand payload has an invalid length ({0})")]
    BadRequest(usize),
}

#[derive(Debug)]
pub struct TimeSyncComponent;

impl TimeSyncComponent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TimeSyncComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for TimeSyncComponent {
    type Error = TimeSyncError;

    fn id(&self) -> DeviceId {
        DeviceId::TimeSync
    }

    async fn run_once(&mut self, _ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        embassy_futures::yield_now().await;
        Ok(())
    }

    async fn handle_tc(
        &mut self,
        ctx: &mut orbisat::ContextHandle<'_>,
        tc: orbipacket::TcPacket,
    ) -> Result<(), Self::Error> {
        let t1 = Instant::now().as_micros();
        let payload = tc.payload().as_bytes();

        if payload.len() != 8 {
            return Err(TimeSyncError::BadRequest(payload.len()));
        }

        let t0 = u64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7],
        ]);
        let t2 = Instant::now().as_micros();

        let mut payload = [0u8; 3 * 8];
        payload[..8].copy_from_slice(&t0.to_le_bytes());
        payload[8..16].copy_from_slice(&t1.to_le_bytes());
        payload[16..24].copy_from_slice(&t2.to_le_bytes());

        ctx.send_outbound(
            DeviceId::TimeSync,
            Payload::from_raw_bytes(payload).unwrap(),
        )
        .map_err(Into::<CommunicationError>::into)?
        .await;

        Ok(())
    }
}
