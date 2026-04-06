use bme280::i2c::AsyncBME280;
use embassy_sync::{blocking_mutex::raw::RawMutex, mutex::Mutex};
use embedded_hal_async::{delay::DelayNs, i2c};
use orbipacket::{DeviceId, TimestampError};
use orbisat::{
    Component, ContextHandle,
    comms::CommunicationError,
    sensor::{
        Sensor,
        readings::{Humidity, Pressure, Temperature},
    },
};

#[derive(thiserror::Error, Debug)]
pub enum Bme280Error<I2C: i2c::ErrorType> {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error("a measurement is needed but values are still available")]
    UnusedMeasurement,
    #[error("i2c error: {0:?}")]
    I2c(bme280::Error<I2C::Error>),
}

impl<I2C: i2c::ErrorType> From<bme280::Error<I2C::Error>> for Bme280Error<I2C> {
    fn from(value: bme280::Error<I2C::Error>) -> Self {
        Self::I2c(value)
    }
}

impl<I2C: i2c::ErrorType> From<TimestampError> for Bme280Error<I2C> {
    fn from(value: TimestampError) -> Self {
        CommunicationError::from(value).into()
    }
}

#[derive(Debug)]
pub struct Bme280Device<I2C>
where
    I2C: i2c::I2c,
{
    driver: AsyncBME280<I2C>,
    latest_temperature: Option<Temperature>,
    latest_pressure: Option<Pressure>,
    latest_humidity: Option<Humidity>,
}

impl<I2C> Bme280Device<I2C>
where
    I2C: i2c::I2c,
{
    pub fn new(i2c: I2C) -> Self {
        let driver = AsyncBME280::new_primary(i2c);
        Self {
            driver,
            latest_temperature: None,
            latest_pressure: None,
            latest_humidity: None,
        }
    }

    pub async fn init<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), bme280::Error<I2C::Error>> {
        self.driver.init(delay).await
    }

    pub async fn get_temperature_measurement<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Temperature, Bme280Error<I2C>> {
        if self.latest_temperature.is_none() {
            if self.latest_pressure.is_some() || self.latest_humidity.is_some() {
                return Err(Bme280Error::UnusedMeasurement);
            } else {
                self.measure(delay).await?;
            }
        }

        // Unwrapping is safe because latest_temperature must be Some at this point
        Ok(self.latest_temperature.take().unwrap())
    }

    pub async fn get_pressure_measurement<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Pressure, Bme280Error<I2C>> {
        if self.latest_pressure.is_none() {
            if self.latest_temperature.is_some() || self.latest_humidity.is_some() {
                return Err(Bme280Error::UnusedMeasurement);
            } else {
                self.measure(delay).await?;
            }
        }

        // Unwrapping is safe because latest_pressure must be Some at this point
        Ok(self.latest_pressure.take().unwrap())
    }

    pub async fn get_humidity_measurement<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<Humidity, Bme280Error<I2C>> {
        if self.latest_humidity.is_none() {
            if self.latest_temperature.is_some() || self.latest_pressure.is_some() {
                return Err(Bme280Error::UnusedMeasurement);
            } else {
                self.measure(delay).await?;
            }
        }

        // Unwrapping is safe because latest_humidity must be Some at this point
        Ok(self.latest_humidity.take().unwrap())
    }

    async fn measure<D: DelayNs>(&mut self, delay: &mut D) -> Result<(), Bme280Error<I2C>> {
        let measurement = self.driver.measure(delay).await?;
        self.latest_temperature = Some(measurement.temperature.into());
        self.latest_pressure = Some(measurement.pressure.into());
        self.latest_humidity = Some(measurement.humidity.into());
        Ok(())
    }
}

#[derive(Debug)]
pub struct Bme280TemperatureSensor<I2C: i2c::I2c, M: RawMutex> {
    inner: Mutex<M, Bme280Device<I2C>>,
}

impl<I2C: i2c::I2c, M: RawMutex> Bme280TemperatureSensor<I2C, M> {
    pub fn new(inner: Mutex<M, Bme280Device<I2C>>) -> Self {
        Self { inner }
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Sensor<Temperature>
    for Bme280TemperatureSensor<I2C, M>
{
    type Error = Bme280Error<I2C>;

    async fn read(&mut self, ctx: &mut ContextHandle<'_>) -> Result<Temperature, Self::Error> {
        self.inner
            .lock()
            .await
            .get_temperature_measurement(ctx.delay_mut())
            .await
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Component for Bme280TemperatureSensor<I2C, M> {
    type Error = Bme280Error<I2C>;

    fn id(&self) -> DeviceId {
        DeviceId::TemperatureSensor
    }

    fn run_once(
        &mut self,
        ctx: &mut orbisat::ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <Self as Sensor<_>>::run_once(self, ctx, self.id())
    }
}

#[derive(Debug)]
pub struct Bme280PressureSensor<I2C: i2c::I2c, M: RawMutex> {
    inner: Mutex<M, Bme280Device<I2C>>,
}

impl<I2C: i2c::I2c, M: RawMutex> Bme280PressureSensor<I2C, M> {
    pub fn new(inner: Mutex<M, Bme280Device<I2C>>) -> Self {
        Self { inner }
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Sensor<Pressure>
    for Bme280PressureSensor<I2C, M>
{
    type Error = Bme280Error<I2C>;

    async fn read(&mut self, ctx: &mut ContextHandle<'_>) -> Result<Pressure, Self::Error> {
        self.inner
            .lock()
            .await
            .get_pressure_measurement(ctx.delay_mut())
            .await
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Component for Bme280PressureSensor<I2C, M> {
    type Error = Bme280Error<I2C>;

    fn id(&self) -> DeviceId {
        DeviceId::PressureSensor
    }

    fn run_once(
        &mut self,
        ctx: &mut orbisat::ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <Self as Sensor<_>>::run_once(self, ctx, self.id())
    }
}

#[derive(Debug)]
pub struct Bme280HumiditySensor<I2C: i2c::I2c, M: RawMutex> {
    inner: Mutex<M, Bme280Device<I2C>>,
}

impl<I2C: i2c::I2c, M: RawMutex> Bme280HumiditySensor<I2C, M> {
    pub fn new(inner: Mutex<M, Bme280Device<I2C>>) -> Self {
        Self { inner }
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Sensor<Humidity>
    for Bme280HumiditySensor<I2C, M>
{
    type Error = Bme280Error<I2C>;

    async fn read(&mut self, ctx: &mut ContextHandle<'_>) -> Result<Humidity, Self::Error> {
        self.inner
            .lock()
            .await
            .get_humidity_measurement(ctx.delay_mut())
            .await
    }
}

impl<I2C: i2c::I2c + core::fmt::Debug, M: RawMutex> Component for Bme280HumiditySensor<I2C, M> {
    type Error = Bme280Error<I2C>;

    fn id(&self) -> DeviceId {
        DeviceId::PressureSensor
    }

    fn run_once(
        &mut self,
        ctx: &mut orbisat::ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        <Self as Sensor<_>>::run_once(self, ctx, self.id())
    }
}
