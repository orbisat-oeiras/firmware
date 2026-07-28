use core::{error::Error, fmt::Display};

use crate::{
    channels::{
        InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
        OutboundPacketChannelPublisher, SdRequestChannel, SdRequestChannelReceiver,
        SdRequestChannelSender,
    },
    sd::SdRequest,
};
use embassy_sync::pubsub::{WaitResult, subscriber::SubscriberWaitFuture};
use embassy_time::{Delay, Duration, Instant, Ticker, Timer};
use heapless::String;
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
    sd_requests: SdRequestChannel,
    tick_duration: Duration,
    delay: Delay,
}

impl Context {
    pub fn new(
        inbound: InboundPacketChannel,
        outbound: OutboundPacketChannel,
        sd_requests: SdRequestChannel,
        tick_duration: Duration,
        delay: Delay,
    ) -> Self {
        Self {
            inbound,
            outbound,
            sd_requests,
            tick_duration,
            delay,
        }
    }

    pub fn to_handle(&self) -> Result<ContextHandle<'_>, ContextError> {
        Ok(ContextHandle {
            inbound: self.inbound.subscriber()?,
            outbound: self.outbound.publisher()?,
            sd_requests: self.sd_requests.sender(),
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

    pub fn sd_request_receiver(&self) -> SdRequestChannelReceiver<'_> {
        self.sd_requests.receiver()
    }
}

#[derive(Debug)]
pub struct ContextHandle<'a> {
    inbound: InboundPacketChannelSubscriber<'a>,
    outbound: OutboundPacketChannelPublisher<'a>,
    sd_requests: SdRequestChannelSender<'a>,
    tick: Ticker,
    delay: Delay,
}

impl<'a> ContextHandle<'a> {
    pub fn receive_inbound<'s>(
        &'s mut self,
    ) -> SubscriberWaitFuture<'s, 'a, InboundPacketChannel, Packet> {
        self.inbound.next_message()
    }

    pub fn receive_inbound_immediate(&mut self) -> Option<WaitResult<Packet>> {
        self.inbound.try_next_message()
    }

    pub async fn send_outbound_raw(&self, message: Packet) {
        self.outbound.publish(message).await;
        // If the channel is full, just ignore it.
        // This prevents the main core from blocking
        // if anything goes wrong on the second core.
        // However, wait 50 us to avoid dropping packets
        // if the channel just happened to be full at
        // this specific instant.
        embassy_futures::select::select(
            Timer::after(Duration::from_micros(50)),
            self.sd_requests.send(SdRequest::WritePacket(message)),
        )
        .await;
    }

    pub async fn send_outbound(
        &self,
        id: DeviceId,
        payload: Payload,
    ) -> Result<(), TimestampError> {
        self.send_outbound_raw(Packet::TmPacket(TmPacket::new(
            id,
            Timestamp::new(Instant::now().as_micros())?,
            payload,
        )))
        .await;

        Ok(())
    }

    pub async fn log(&self, message: String<128>) {
        self.sd_requests.send(SdRequest::LogMessage(message)).await;
    }

    pub async fn sd_request(&self, request: SdRequest) {
        self.sd_requests.send(request).await;
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
