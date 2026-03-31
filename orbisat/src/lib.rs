#![no_std]

use core::{error::Error, fmt::Display};

use embassy_sync::pubsub::{publisher::PublisherWaitFuture, subscriber::SubscriberWaitFuture};
use orbipacket::{DeviceId, Packet};
use orbisat_firmware_config::packet_channel::{
    InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
    OutboundPacketChannelPublisher,
};

pub mod comms;

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
}

impl Context {
    pub fn new(inbound: InboundPacketChannel, outbound: OutboundPacketChannel) -> Self {
        Self { inbound, outbound }
    }

    pub fn to_handle(&self) -> Result<ContextHandle<'_>, ContextError> {
        Ok(ContextHandle {
            inbound: self.inbound.subscriber()?,
            outbound: self.outbound.publisher()?,
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
}

impl<'a> ContextHandle<'a> {
    pub fn receive_inbound<'s>(
        &'s mut self,
    ) -> SubscriberWaitFuture<'s, 'a, InboundPacketChannel, Packet> {
        self.inbound.next_message()
    }

    pub fn send_outbound<'s>(
        &'s self,
        message: Packet,
    ) -> PublisherWaitFuture<'s, 'a, OutboundPacketChannel, Packet> {
        self.outbound.publish(message)
    }
}

pub trait Component {
    type Error: core::error::Error;

    fn id(&self) -> DeviceId;

    fn run(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            loop {
                self.run_once(ctx).await?;
            }
        }
    }

    fn run_once(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
