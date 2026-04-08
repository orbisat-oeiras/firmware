#![no_std]

pub mod packet_channel {
    pub type Mutex = embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

    pub const COMPONENT_COUNT: usize = 10;
    pub const SINK_COUNT: usize = 2;
    pub const CAP: usize = 4;

    pub type InboundPacketChannel =
        embassy_sync::pubsub::PubSubChannel<Mutex, orbipacket::Packet, CAP, COMPONENT_COUNT, 1>;
    pub type InboundPacketChannelSubscriber<'a> =
        embassy_sync::pubsub::Subscriber<'a, Mutex, orbipacket::Packet, CAP, COMPONENT_COUNT, 1>;
    pub type InboundPacketChannelPublisher<'a> =
        embassy_sync::pubsub::Publisher<'a, Mutex, orbipacket::Packet, CAP, COMPONENT_COUNT, 1>;

    pub type OutboundPacketChannel = embassy_sync::pubsub::PubSubChannel<
        Mutex,
        orbipacket::Packet,
        CAP,
        SINK_COUNT,
        COMPONENT_COUNT,
    >;
    pub type OutboundPacketChannelSubscriber<'a> = embassy_sync::pubsub::Subscriber<
        'a,
        Mutex,
        orbipacket::Packet,
        CAP,
        SINK_COUNT,
        COMPONENT_COUNT,
    >;
    pub type OutboundPacketChannelPublisher<'a> = embassy_sync::pubsub::Publisher<
        'a,
        Mutex,
        orbipacket::Packet,
        CAP,
        SINK_COUNT,
        COMPONENT_COUNT,
    >;
}
