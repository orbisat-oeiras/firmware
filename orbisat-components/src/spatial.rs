use embedded_hal::i2c;
use mma8x5x::{
    Error as DriverError, GScale, Mma8x5x, ModeChangeError, OutputDataRate, PowerMode, SlaveAddr,
    ic::Mma8452,
    mode::{Active, Standby},
};
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
