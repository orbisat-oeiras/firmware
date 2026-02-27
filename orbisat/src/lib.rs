#![no_std]

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};
use orbipacket::Packet;

pub trait Component {
    fn run(
        &self,
        recv: Receiver<'static, CriticalSectionRawMutex, Packet, 256>,
    ) -> impl core::future::Future<Output = ()> + Send;
}

pub trait PacketSink {
    fn sink_bytes(&self, buf: &[u8]) -> impl core::future::Future<Output = ()> + Send;
}

impl<T> Component for T
where
    T: PacketSink + Sync,
{
    async fn run(&self, recv: Receiver<'static, CriticalSectionRawMutex, Packet, 256>) {
        let mut buf = [0u8; Packet::MAX_ENCODE_BUFFER_SIZE];

        loop {
            let packet = recv.receive().await;
            let packet = packet.encode(&mut buf).unwrap();
            self.sink_bytes(packet).await;
        }
    }
}
