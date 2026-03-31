use embassy_sync::pubsub::WaitResult;
use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};
use orbisat_firmware_config::packet_channel::{
    InboundPacketChannelPublisher, OutboundPacketChannelSubscriber,
};

use crate::Component;

pub trait ByteSink {
    fn sink(&mut self, buf: &[u8]) -> impl Future<Output = ()>;
}

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
    fn id(&self) -> DeviceId {
        DeviceId::System
    }

    async fn run_once(&mut self) {
        let packet = match self.recv.next_message().await {
            WaitResult::Lagged(n) => {
                defmt::info!("PacketSink dropped {} packets", n);
                return;
            }
            WaitResult::Message(p) => p,
        };
        let packet = packet.encode(&mut self.buf).unwrap();
        self.sink.sink(packet).await;
    }
}

pub trait ByteSource {
    fn fill(&mut self, buf: &mut [u8]) -> impl Future<Output = usize>;
}

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
    fn id(&self) -> DeviceId {
        DeviceId::System
    }

    async fn run_once(&mut self) {
        let filled = self.source.fill(&mut self.buf[self.buf_index..]).await;

        let (remaining, packets) = match Packet::decode_stateless(
            &mut self.buf[..self.buf_index + filled],
            &mut self.packet_buf,
        ) {
            Ok(r) => r,
            Err(e) => {
                defmt::error!("{}", e);
                panic!()
            }
        };

        let idx =
            (remaining.as_ptr() as usize - self.buf.as_ptr() as usize) / core::mem::size_of::<u8>();
        self.buf.rotate_left(idx);
        self.buf_index += filled - idx;

        for packet in packets {
            self.send.publish(*packet).await;
        }
    }
}
