#![no_std]

pub mod packet_channel {
    pub type Mutex = embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    pub type Message = orbipacket::Packet;
    pub const CAP: usize = 4;
    pub const SUBS: usize = 2;
    pub const PUBS: usize = 1;

    pub type PacketChannel = embassy_sync::pubsub::PubSubChannel<Mutex, Message, CAP, SUBS, PUBS>;
    pub type PacketChannelSubscriber<'a> =
        embassy_sync::pubsub::Subscriber<'a, Mutex, Message, CAP, SUBS, PUBS>;
    pub type PacketChannelPublisher<'a> =
        embassy_sync::pubsub::Publisher<'a, Mutex, Message, CAP, SUBS, PUBS>;
}
