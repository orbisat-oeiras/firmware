#![no_std]
#![feature(with_negative_coherence)]

pub mod primary;
pub mod sd;
pub mod secondary;
pub mod spatial;

use core::convert::Infallible;

use embassy_time::Instant;
use orbipacket::{DeviceId, Payload};
use orbisat::{
    Component, Status,
    comms::{ByteSink, ByteSource, CommunicationError},
};

#[derive(Debug)]
pub struct ConsoleByteSink;

impl ByteSink for ConsoleByteSink {
    type Error = Infallible;

    async fn sink(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        defmt::info!("Outbound packet: {:02X}", buf);
        Ok(())
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
    type Error = W::Error;

    async fn sink(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        defmt::info!("Uart sending");
        self.0.write_all(buf).await?;
        self.0.flush().await?;
        Ok(())
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
    type Error = R::Error;

    async fn fill(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf).await
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
pub struct TimeSyncComponent {
    status: Status,
    bootcount: u8,
}

impl TimeSyncComponent {
    pub fn new(bootcount: u8) -> Self {
        Self {
            bootcount,
            status: Status::Initialized,
        }
    }
}

impl Component for TimeSyncComponent {
    type Error = TimeSyncError;

    fn id(&self) -> DeviceId {
        DeviceId::TimeSync
    }

    fn status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
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
        let received_payload = tc.payload().as_bytes();

        match received_payload.len() {
            2 if received_payload[..2] == *b"BC" => {
                ctx.send_outbound(DeviceId::TimeSync, Payload::from_u8(self.bootcount))
                    .map_err(CommunicationError::from)?
                    .await;

                Ok(())
            }
            8 => {
                let t2 = Instant::now().as_micros();

                let mut payload = [0u8; 3 * 8];
                payload[..8].copy_from_slice(&received_payload[..8]);
                payload[8..16].copy_from_slice(&t1.to_le_bytes());
                payload[16..24].copy_from_slice(&t2.to_le_bytes());

                ctx.send_outbound(
                    DeviceId::TimeSync,
                    // Unwrapping is safe because payload is 24 bytes long
                    Payload::from_raw_bytes(payload).unwrap(),
                )
                .map_err(CommunicationError::from)?
                .await;

                Ok(())
            }
            _ => Err(TimeSyncError::BadRequest(received_payload.len())),
        }
    }
}
