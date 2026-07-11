use core::{fmt::Debug, str::Utf8Error};

use embedded_hal::i2c;
use mma8x5x::{
    Error as DriverError, GScale, Mma8x5x, ModeChangeError, OutputDataRate, PowerMode, SlaveAddr,
    ic::Mma8452,
    mode::{Active, Standby},
};
use nmea::Nmea;
use orbipacket::{DeviceId, TimestampError};
use orbisat::{
    Component,
    comms::CommunicationError,
    sensor::{Sensor, readings::Acceleration},
};

#[derive(thiserror::Error, Debug)]
pub enum Mma8452Error<I2C: i2c::I2c> {
    #[error("driver error: {0:?}")]
    Driver(DriverError<<I2C as i2c::ErrorType>::Error>),
    #[error("failed to change Mma8452 to active mode: {0:?}")]
    ModeChange(ModeChangeError<<I2C as i2c::ErrorType>::Error, Mma8x5x<I2C, Mma8452, Standby>>),
    #[error(transparent)]
    Communication(#[from] CommunicationError),
}

impl<I2C: i2c::I2c> From<DriverError<<I2C as i2c::ErrorType>::Error>> for Mma8452Error<I2C> {
    fn from(value: DriverError<<I2C as i2c::ErrorType>::Error>) -> Self {
        Self::Driver(value)
    }
}

impl<I2C: i2c::I2c>
    From<ModeChangeError<<I2C as i2c::ErrorType>::Error, Mma8x5x<I2C, Mma8452, Standby>>>
    for Mma8452Error<I2C>
{
    fn from(
        value: ModeChangeError<<I2C as i2c::ErrorType>::Error, Mma8x5x<I2C, Mma8452, Standby>>,
    ) -> Self {
        Self::ModeChange(value)
    }
}

impl<I2C: i2c::I2c> From<TimestampError> for Mma8452Error<I2C> {
    fn from(value: TimestampError) -> Self {
        Self::Communication(CommunicationError::Timestamp(value))
    }
}

pub struct Mma8542Component<I2C: i2c::I2c> {
    driver: Mma8x5x<I2C, Mma8452, Active>,
}

impl<I2C: i2c::I2c> Mma8542Component<I2C> {
    pub fn new(i2c: I2C) -> Result<Self, Mma8452Error<I2C>> {
        let mut driver = Mma8x5x::new_mma8452(i2c, SlaveAddr::Alternative(true));

        driver.set_scale(GScale::G8)?;
        driver.set_data_rate(OutputDataRate::Hz6_25)?;
        driver.set_wake_power_mode(PowerMode::LowNoiseLowPower)?;

        let driver = driver.into_active()?;
        Ok(Self { driver })
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug> Sensor<Acceleration> for Mma8542Component<I2C> {
    type Error = Mma8452Error<I2C>;

    async fn read(
        &mut self,
        _ctx: &mut orbisat::ContextHandle<'_>,
    ) -> Result<Acceleration, Self::Error> {
        let measurement = self.driver.read()?;

        Ok(Acceleration::new(
            measurement.x,
            measurement.y,
            measurement.z,
        ))
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug> Component for Mma8542Component<I2C> {
    type Error = Mma8452Error<I2C>;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Accelerometer
    }

    fn run_once(
        &mut self,
        ctx: &mut orbisat::ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <Self as Sensor<_>>::run_once(self, ctx, self.id())
    }
}

#[derive(thiserror::Error)]
pub enum GnssError<R: embedded_io_async::Read> {
    #[error("Uart error: {0:?}")]
    Uart(R::Error),
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error(transparent)]
    Utf8(#[from] Utf8Error),
    #[error("Nema parsing error")]
    Nmea,
}

impl<R: embedded_io_async::Read> From<nmea::Error<'_>> for GnssError<R> {
    fn from(_: nmea::Error<'_>) -> Self {
        Self::Nmea
    }
}

impl<R: embedded_io_async::Read> Debug for GnssError<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Uart(arg0) => f.debug_tuple("Uart").field(arg0).finish(),
            Self::Communication(arg0) => f.debug_tuple("Communication").field(arg0).finish(),
            Self::Utf8(arg0) => f.debug_tuple("Utf8").field(arg0).finish(),
            Self::Nmea => write!(f, "Nmea"),
        }
    }
}

#[derive(Debug)]
pub struct GnssComponent<R: embedded_io_async::Read> {
    uart: R,
    nmea: Nmea,
    buf: [u8; nmea::SENTENCE_MAX_LEN],
    trailing_index: usize,
}

impl<R: embedded_io_async::Read> GnssComponent<R> {
    pub fn new(uart: R) -> Self {
        let nmea = Nmea::default();

        Self {
            uart,
            nmea,
            buf: [0u8; _],
            trailing_index: 0,
        }
    }
}

impl<R: embedded_io_async::Read> Component for GnssComponent<R> {
    type Error = GnssError<R>;

    fn id(&self) -> DeviceId {
        DeviceId::Gps
    }

    async fn run_once(&mut self, _ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        defmt::warn!("BUF: {}", self.buf);

        let read = self
            .uart
            .read(&mut self.buf[self.trailing_index..])
            .await
            .map_err(|e| GnssError::Uart(e))?;

        let mut carriage_return = false;

        defmt::warn!("BUF: {}", self.buf);

        for idx in 0..self.trailing_index + read {
            match self.buf[idx] {
                b'\r' => carriage_return = true,
                b'\n' if carriage_return => {
                    let _ = self.nmea.parse(str::from_utf8(&self.buf[..idx])?)?;
                    carriage_return = false;

                    self.buf.rotate_left(idx);
                    self.trailing_index = 0;
                }
                _ => self.trailing_index += 1,
            }
        }

        defmt::warn!("BUF: {}", self.buf);

        Ok(())
    }
}
