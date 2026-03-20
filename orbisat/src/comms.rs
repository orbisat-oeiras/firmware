use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};
use orbipacket::Packet;

use crate::Component;

pub trait ByteSink {
    fn sink(&mut self, buf: &[u8]) -> impl Future<Output = ()>;
}

pub struct PacketSink<S>
where
    S: ByteSink,
{
    recv: Receiver<'static, CriticalSectionRawMutex, Packet, 1>,
    sink: S,
    buf: [u8; Packet::MAX_ENCODE_BUFFER_SIZE],
}

impl<S> PacketSink<S>
where
    S: ByteSink,
{
    pub fn new(
        recv: Receiver<'static, CriticalSectionRawMutex, Packet, 1>,
        sink: S,
    ) -> PacketSink<S> {
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
    async fn run(&mut self) {
        loop {
            let packet = self.recv.receive().await;
            let packet = packet.encode(&mut self.buf).unwrap();
            self.sink.sink(packet).await;
        }
    }
}
