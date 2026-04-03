#![no_std]

use core::{error::Error, fmt::Display};

use embassy_sync::pubsub::{
    WaitResult, publisher::PublisherWaitFuture, subscriber::SubscriberWaitFuture,
};
use embassy_time::Instant;
use orbipacket::{DeviceId, Packet, Payload, TcPacket, Timestamp, TimestampError, TmPacket};
use orbisat_firmware_config::packet_channel::{
    InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
    OutboundPacketChannelPublisher,
};

use crate::comms::CommunicationError;

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
}

pub trait Component {
    type Error: core::error::Error + From<CommunicationError>;

    fn id(&self) -> DeviceId;

    fn run(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            defmt::info!("Component {} started running", self.id());
            loop {
                self.receive_tc(ctx).await?;
                self.run_once(ctx).await?;
            }
        }
    }

    fn receive_tc(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            while let Some(r) = ctx.receive_inbound_immediate() {
                let packet = match r {
                    WaitResult::Lagged(n) => {
                        return Err(CommunicationError::InboundPacketLagged(n).into());
                    }
                    WaitResult::Message(p) => p,
                };

                match packet {
                    Packet::TmPacket(_) => {
                        return Err(CommunicationError::InboundTmPacket.into());
                    }
                    Packet::TcPacket(tc_packet) => {
                        if *tc_packet.device_id() == self.id() {
                            self.handle_tc(ctx, tc_packet).await?;
                        }
                    }
                };
            }

            Ok(())
        }
    }

    fn run_once(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn handle_tc(
        &mut self,
        _ctx: &mut ContextHandle<'_>,
        _tc: TcPacket,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async { Ok(()) }
    }
}
