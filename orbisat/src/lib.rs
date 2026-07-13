#![no_std]
#![feature(negative_impls)]

use core::{error::Error, fmt::Display};

use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    mutex::MutexGuard,
    pubsub::{WaitResult, publisher::PublisherWaitFuture, subscriber::SubscriberWaitFuture},
};
use embassy_time::{Delay, Duration, Instant, Ticker};
use orbipacket::{DeviceId, Packet, Payload, TcPacket, Timestamp, TimestampError, TmPacket};
use orbisat_firmware_config::packet_channel::{
    AsyncMutex, InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
    OutboundPacketChannelPublisher,
};

use crate::comms::CommunicationError;

pub mod comms;
pub mod sensor;

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
    mutex: AsyncMutex,
}

impl Context {
    pub fn new(
        inbound: InboundPacketChannel,
        outbound: OutboundPacketChannel,
        tick_duration: Duration,
        delay: Delay,
        mutex: AsyncMutex,
    ) -> Self {
        Self {
            inbound,
            outbound,
            tick_duration,
            delay,
            mutex,
        }
    }

    pub fn to_handle(&self) -> Result<ContextHandle<'_>, ContextError> {
        Ok(ContextHandle {
            inbound: self.inbound.subscriber()?,
            outbound: self.outbound.publisher()?,
            tick: Ticker::every(self.tick_duration),
            delay: self.delay.clone(),
            mutex: &self.mutex,
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
    mutex: &'a AsyncMutex,
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

    pub async fn lock(&self) -> MutexGuard<'_, CriticalSectionRawMutex, ()> {
        self.mutex.lock().await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Initialized = 0,
    Running = 1,
    Paused = 2,
    Failed = 3,
}

impl Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Status::Initialized => write!(f, "initialized"),
            Status::Running => write!(f, "running"),
            Status::Paused => write!(f, "paused"),
            Status::Failed => write!(f, "failed"),
        }
    }
}

const STATUS_REPORT_COMMAND: &[u8; 2] = b"SR";

pub trait Component {
    type Error: core::error::Error + From<CommunicationError>;

    fn id(&self) -> DeviceId;

    fn status(&self) -> Status;

    fn set_status(&mut self, status: Status);

    fn run(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            defmt::info!("Component {} started running", self.id());
            self.set_status(Status::Running);

            loop {
                let result = {
                    self.receive_tc(ctx).await?;
                    self.run_once(ctx).await
                };

                match result {
                    Ok(_) => {}
                    Err(e) => {
                        self.set_status(Status::Failed);
                        return Err(e);
                    }
                }
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
                            if tc_packet.payload().as_bytes() == STATUS_REPORT_COMMAND {
                                self.status_report(ctx).await?;
                            } else {
                                self.handle_tc(ctx, tc_packet).await?;
                            }
                        }
                    }
                };
            }

            Ok(())
        }
    }

    fn status_report(&self, ctx: &ContextHandle) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            // Unwraping is safe here because only one byte is provided
            let payload = Payload::from_raw_bytes([self.status() as u8]).unwrap();
            ctx.send_outbound(self.id(), payload)
                .map_err(CommunicationError::from)?
                .await;

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
