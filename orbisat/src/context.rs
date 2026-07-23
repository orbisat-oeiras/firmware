use core::{error::Error, fmt::Display};

use crate::channels::{
    InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
    OutboundPacketChannelPublisher,
};
use embassy_sync::pubsub::{
    WaitResult, publisher::PublisherWaitFuture, subscriber::SubscriberWaitFuture,
};
use embassy_time::{Delay, Duration, Instant, Ticker};
use orbipacket::{DeviceId, Packet, Payload, Timestamp, TimestampError, TmPacket};

#[derive(Debug)]
pub enum ContextError {
    ChannelError(embassy_sync::pubsub::Error),
}

impl Display for ContextError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContextError::ChannelError(error) => match error {
                embassy_sync::pubsub::Error::MaximumSubscribersReached => {
                    write!(f, "channel reached maximum subscriber count")
                }
                embassy_sync::pubsub::Error::MaximumPublishersReached => {
                    write!(f, "channel reached maximum publisher count")
                }
            },
        }
    }
}

impl Error for ContextError {}

impl From<embassy_sync::pubsub::Error> for ContextError {
    fn from(value: embassy_sync::pubsub::Error) -> Self {
        Self::ChannelError(value)
    }
}

#[derive(Debug)]
pub struct Context {
    inbound: InboundPacketChannel,
    outbound: OutboundPacketChannel,
    tick_duration: Duration,
    delay: Delay,
}

impl Context {
    pub fn new(
        inbound: InboundPacketChannel,
        outbound: OutboundPacketChannel,
        tick_duration: Duration,
        delay: Delay,
    ) -> Self {
        Self {
            inbound,
            outbound,
            tick_duration,
            delay,
        }
    }

    pub fn to_handle(&self) -> Result<ContextHandle<'_>, ContextError> {
        Ok(ContextHandle {
            inbound: self.inbound.subscriber()?,
            outbound: self.outbound.publisher()?,
            tick: Ticker::every(self.tick_duration),
            delay: self.delay.clone(),
        })
    }

    pub fn inbound(&self) -> &InboundPacketChannel {
        &self.inbound
    }

    pub fn outbound(&self) -> &OutboundPacketChannel {
        &self.outbound
    }
}

#[derive(Debug)]
pub struct ContextHandle<'a> {
    inbound: InboundPacketChannelSubscriber<'a>,
    outbound: OutboundPacketChannelPublisher<'a>,
    tick: Ticker,
    delay: Delay,
}

impl<'a> ContextHandle<'a> {
    pub fn receive_inbound<'s>(
        &'s mut self,
    ) -> SubscriberWaitFuture<'s, 'a, InboundPacketChannel, Packet> {
        self.inbound.next_message()
    }

    pub fn send_outbound_raw<'s>(
        &'s self,
        message: Packet,
    ) -> PublisherWaitFuture<'s, 'a, OutboundPacketChannel, Packet> {
        self.outbound.publish(message)
    }

    pub fn send_outbound<'s>(
        &'s self,
        id: DeviceId,
        payload: Payload,
    ) -> Result<PublisherWaitFuture<'s, 'a, OutboundPacketChannel, Packet>, TimestampError> {
        Ok(self.send_outbound_raw(Packet::TmPacket(TmPacket::new(
            id,
            Timestamp::new(Instant::now().as_micros())?,
            payload,
        ))))
    }

    pub fn receive_inbound_immediate(&mut self) -> Option<WaitResult<Packet>> {
        self.inbound.try_next_message()
    }

    pub fn delay_mut(&mut self) -> &mut Delay {
        &mut self.delay
    }

    pub fn next_tick(&mut self) -> impl Future<Output = ()> + Send + Sync + '_ {
        self.tick.next()
    }

    pub fn timestamp(&self) -> Result<Timestamp, TimestampError> {
        Timestamp::new(Instant::now().as_micros())
    }
}
