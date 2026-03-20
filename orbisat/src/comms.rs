use embassy_sync::pubsub::WaitResult;
use orbipacket::Packet;
use orbisat_firmware_config::packet_channel::PacketChannelSubscriber;

use crate::Component;

pub trait ByteSink {
    fn sink(&mut self, buf: &[u8]) -> impl Future<Output = ()>;
}

pub struct PacketSink<S>
where
    S: ByteSink,
{
    recv: PacketChannelSubscriber<'static>,
    sink: S,
    buf: [u8; Packet::MAX_ENCODE_BUFFER_SIZE],
}

impl<S> PacketSink<S>
where
    S: ByteSink,
{
    pub fn new(recv: PacketChannelSubscriber<'static>, sink: S) -> PacketSink<S> {
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
