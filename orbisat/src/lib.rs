#![no_std]
#![feature(negative_impls)]

use core::fmt::Display;

use embassy_sync::pubsub::WaitResult;
use orbipacket::{DeviceId, Packet, Payload, TcPacket};

use crate::{comms::CommunicationError, context::ContextHandle};

pub mod channels;
pub mod comms;
pub mod context;
pub mod sd;
pub mod sensor;

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
                .await
                .map_err(CommunicationError::from)?;

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
