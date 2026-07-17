use embassy_time::{Duration, Timer};
use embedded_hal::spi::SpiDevice;
use orbipacket::{DeviceId, TimestampError};
use orbisat::{Component, comms::CommunicationError};

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
    pwm: PWM,
    data: SweepData<'a>,
    forward_sweep: bool,
    writer: SdFileWriter<'a, SPI>,
}

impl<'a, PWM: SetFrequency, SPI: SpiDevice<u8>> SpeakerComponent<'a, PWM, SPI> {
    pub fn new(pwm: PWM, data: SweepData<'a>, writer: SdFileWriter<'a, SPI>) -> Self {
        Self {
            pwm,
            data,
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

    async fn run_once(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        if self.forward_sweep {
            self.writer.write(b"FORWARD SWEEP TIMESTAMP ", ctx).await?;
            self.writer
                .write(&ctx.timestamp()?.get().to_le_bytes(), ctx)
                .await?;
            self.writer.write(b"\n", ctx).await?;
            defmt::info!("Starting forward sweep");
            for (frequency, duration) in self.data {
                self.pwm.set_frequency(*frequency).await;
                Timer::after(Duration::from_micros(*duration)).await;
            }
            self.forward_sweep = false;
        } else {
            self.writer.write(b"REVERSE SWEEP TIMESTAMP ", ctx).await?;
            self.writer
                .write(&ctx.timestamp()?.get().to_le_bytes(), ctx)
                .await?;
            self.writer.write(b"\n", ctx).await?;
            defmt::info!("Starting reverse sweep");
            for (frequency, duration) in self.data.iter().rev() {
                self.pwm.set_frequency(*frequency).await;
                Timer::after(Duration::from_micros(*duration)).await;
            }

            self.forward_sweep = true;
        }

        Ok(())
    }
}
