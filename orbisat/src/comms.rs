use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    pubsub::{Subscriber, WaitResult},
};
use orbipacket::Packet;

use crate::Component;

pub trait ByteSink {
    fn sink(&mut self, buf: &[u8]) -> impl Future<Output = ()>;
}

pub struct PacketSink<S>
where
    S: ByteSink,
{
    recv: Subscriber<'static, CriticalSectionRawMutex, Packet, 4, 2, 1>,
    sink: S,
    buf: [u8; Packet::MAX_ENCODE_BUFFER_SIZE],
}

impl<S> PacketSink<S>
where
    S: ByteSink,
{
    pub fn new(
        recv: Subscriber<'static, CriticalSectionRawMutex, Packet, 4, 2, 1>,
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
    // TODO: rewrite run in terms of run_once (also add a default impl in the trait?)
    async fn run(&mut self) {
        loop {
            let packet = match self.recv.next_message().await {
                WaitResult::Lagged(n) => {
                    defmt::info!("PacketSink dropped {} packets", n);
                    continue;
                }
                WaitResult::Message(p) => p,
            };
            let packet = packet.encode(&mut self.buf).unwrap();
            self.sink.sink(packet).await;
        }
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
