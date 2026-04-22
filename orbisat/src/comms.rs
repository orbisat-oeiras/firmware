use embassy_sync::pubsub::WaitResult;
use orbipacket::{
    DeviceId, Packet, Payload, Timestamp, TimestampError, TmPacket, decode::DecodeError,
    encode::EncodeError,
};
use orbisat_firmware_config::packet_channel::{
    InboundPacketChannelPublisher, OutboundPacketChannelSubscriber,
};

use crate::{Component, ContextHandle};

#[derive(thiserror::Error, Debug)]
pub enum CommunicationError {
    #[error("{0} outbound packets were lagged")]
    OutboundPacketLagged(u64),
    #[error("{0} Inbound packets were lagged")]
    InboundPacketLagged(u64),
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("received an inbound telemetry packet")]
    InboundTmPacket,
    #[error(transparent)]
    Timestamp(#[from] TimestampError),
}

impl !embedded_hal::pwm::Error for CommunicationError {}

pub trait ByteSink {
    type Error;

    fn sink(&mut self, buf: &[u8]) -> impl Future<Output = Result<(), Self::Error>>;
}

#[derive(Debug)]
pub struct PacketSink<S>
where
    S: ByteSink,
{
    recv: OutboundPacketChannelSubscriber<'static>,
    sink: S,
    buf: [u8; Packet::MAX_ENCODE_BUFFER_SIZE],
}

impl<S> PacketSink<S>
where
    S: ByteSink,
{
    pub fn new(recv: OutboundPacketChannelSubscriber<'static>, sink: S) -> PacketSink<S> {
        Self {
            recv,
            sink,
            buf: [0; _],
        }
    }
}

impl<S> Component for PacketSink<S>
where
    S: ByteSink,
{
    type Error = CommunicationError;

    fn id(&self) -> DeviceId {
        DeviceId::System
    }

    async fn run_once(&mut self, _ctx: &mut ContextHandle<'_>) -> Result<(), Self::Error> {
        let packet = match self.recv.next_message().await {
            WaitResult::Lagged(n) => {
                return Err(CommunicationError::OutboundPacketLagged(n));
            }
            WaitResult::Message(p) => p,
        };
        let packet = packet.encode(&mut self.buf)?;
        match self.sink.sink(packet).await {
            Ok(_) => {}
            Err(_) => defmt::error!("Byte sink error"),
        }

        Ok(())
    }
}

pub trait ByteSource {
    fn fill(&mut self, buf: &mut [u8]) -> impl Future<Output = usize>;
}

#[derive(Debug)]
pub struct PacketSource<S>
where
    S: ByteSource,
{
    send: InboundPacketChannelPublisher<'static>,
    source: S,
    buf: [u8; 32],
    buf_index: usize,
    packet_buf: [Packet; 16],
}

impl<S> PacketSource<S>
where
    S: ByteSource,
{
    pub fn new(send: InboundPacketChannelPublisher<'static>, source: S) -> Self {
        Self {
            send,
            source,
            buf: [0; _],
            buf_index: 0,
            packet_buf: [Packet::TmPacket(TmPacket::new(
                DeviceId::System,
                // Unwrapping is safe because 0 is a valid timestamp
                Timestamp::new(0).unwrap(),
                Payload::new(),
            )); _],
        }
    }
}

impl<S> Component for PacketSource<S>
where
    S: ByteSource,
{
    type Error = CommunicationError;

    fn id(&self) -> DeviceId {
        DeviceId::System
    }

    async fn run_once(&mut self, _ctx: &mut ContextHandle<'_>) -> Result<(), Self::Error> {
        let filled = self.source.fill(&mut self.buf[self.buf_index..]).await;

        let (remaining, packets) = Packet::decode_stateless(
            &mut self.buf[..self.buf_index + filled],
            &mut self.packet_buf,
        )?;

        let idx =
            (remaining.as_ptr() as usize - self.buf.as_ptr() as usize) / core::mem::size_of::<u8>();
        self.buf.rotate_left(idx);
        self.buf_index += filled - idx;

        for packet in packets {
            self.send.publish(*packet).await;
        }

        Ok(())
    }
}
