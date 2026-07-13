use embassy_time::{Duration, Timer};
use embedded_hal::spi::SpiDevice;
use orbipacket::{DeviceId, TimestampError};
use orbisat::{
    Component, Status,
    comms::{ByteSink, CommunicationError},
};

use crate::sd::{SdError, SdFileWriter};

#[derive(thiserror::Error)]
pub enum SpeakerError<SPI: SpiDevice<u8>> {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error(transparent)]
    Sd(#[from] SdError<SPI>),
}

impl<SPI: SpiDevice<u8>> From<TimestampError> for SpeakerError<SPI> {
    fn from(value: TimestampError) -> Self {
        Self::Communication(CommunicationError::Timestamp(value))
    }
}

impl<SPI: SpiDevice<u8>> core::fmt::Debug for SpeakerError<SPI> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            SpeakerError::Communication(f0) => f.debug_tuple("Communication").field(&f0).finish(),
            SpeakerError::Sd(sd_error) => f.debug_tuple("Sd").field(&sd_error).finish(),
        }
    }
}

pub trait SetFrequency {
    fn set_frequency(&mut self, frequency: u32) -> impl Future<Output = ()>;

    fn stop(&mut self) -> impl Future<Output = ()>;
}

type SweepData<'a> = &'a [(u32, u64)];

pub struct SpeakerComponent<'a, PWM: SetFrequency, SPI: SpiDevice<u8>> {
    status: Status,
    pwm: PWM,
    data: SweepData<'a>,
    idx: usize,
    forward_sweep: bool,
    writer: SdFileWriter<'a, SPI>,
}

impl<'a, PWM: SetFrequency, SPI: SpiDevice<u8>> SpeakerComponent<'a, PWM, SPI> {
    pub fn new(pwm: PWM, data: SweepData<'a>, writer: SdFileWriter<'a, SPI>) -> Self {
        Self {
            status: Status::Initialized,
            pwm,
            data,
            idx: 0,
            forward_sweep: true,
            writer,
        }
    }
}

impl<'a, PWM: SetFrequency, SPI: SpiDevice<u8>> Component for SpeakerComponent<'a, PWM, SPI> {
    type Error = SpeakerError<SPI>;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Mission1
    }

    fn status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    async fn run(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.set_status(Status::Paused);

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

    async fn run_once(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        if self.status == Status::Running {
            self.pwm.set_frequency(self.data[self.idx].0).await;
            Timer::after(Duration::from_micros(self.data[self.idx].1)).await;

            if self.forward_sweep {
                if self.idx >= self.data.len() - 1 {
                    self.writer.sink(b"REVERSE SWEEP TIMESTAMP").await?;
                    self.writer
                        .sink(&ctx.timestamp()?.get().to_le_bytes())
                        .await?;
                    self.writer.sink(b"\n").await?;

                    defmt::info!("Starting reverse sweep");

                    self.forward_sweep = false;
                } else {
                    self.idx += 1;
                }
            } else {
                if self.idx == 0 {
                    self.writer.sink(b"FORWARD SWEEP TIMESTAMP").await?;
                    self.writer
                        .sink(&ctx.timestamp()?.get().to_le_bytes())
                        .await?;
                    self.writer.sink(b"\n").await?;

                    defmt::info!("Starting forward sweep");

                    self.forward_sweep = true;
                } else {
                    self.idx -= 1;
                }
            }
        } else {
            embassy_futures::yield_now().await;
        }

        Ok(())
    }

    async fn handle_tc(
        &mut self,
        _ctx: &mut orbisat::ContextHandle<'_>,
        _tc: orbipacket::TcPacket,
    ) -> Result<(), Self::Error> {
        match self.status {
            Status::Running => {
                self.set_status(Status::Paused);
                self.pwm.stop().await;
            }
            Status::Paused => {
                self.set_status(Status::Running);
            }
            _ => {}
        }

        Ok(())
    }
}
