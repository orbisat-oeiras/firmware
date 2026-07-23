use bme280::i2c::AsyncBME280;
use embassy_sync::{blocking_mutex, mutex::Mutex};
use embedded_hal_async::{delay::DelayNs, i2c};
use heapless::Deque;
use orbipacket::{DeviceId, TimestampError};
use orbisat::{
    Component, ContextHandle, Status,
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
    #[error("buffer for {0} measurements is full")]
    FullBuffer(&'static str),
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
    temperature: Deque<Temperature, 256>,
    pressure: Deque<Pressure, 256>,
    humidity: Deque<Humidity, 256>,
    initialized: bool,
}

impl<I2C> Bme280Device<I2C>
where
    I2C: i2c::I2c,
{
    pub fn new(i2c: I2C) -> Self {
        let driver = AsyncBME280::new_primary(i2c);
        Self {
            driver,
            temperature: Deque::new(),
            pressure: Deque::new(),
            humidity: Deque::new(),
            initialized: false,
        }
    }

    pub async fn init<D: DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), bme280::Error<I2C::Error>> {
        self.driver.init(delay).await?;
        self.initialized = true;
        Ok(())
    }

    pub async fn get_temperature_measurement(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> Result<Temperature, Bme280Error<I2C>> {
        match self.temperature.pop_front() {
            Some(m) => Ok(m),
            None => {
                self.measure(ctx).await?;
                // SAFETY: measure fills the deque
                Ok(unsafe { self.temperature.pop_front_unchecked() })
            }
        }
    }

    pub async fn get_pressure_measurement(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> Result<Pressure, Bme280Error<I2C>> {
        match self.pressure.pop_front() {
            Some(m) => Ok(m),
            None => {
                self.measure(ctx).await?;
                // SAFETY: measure fills the deque
                Ok(unsafe { self.pressure.pop_front_unchecked() })
            }
        }
    }

    pub async fn get_humidity_measurement(
        &mut self,
        ctx: &mut ContextHandle<'_>,
    ) -> Result<Humidity, Bme280Error<I2C>> {
        match self.humidity.pop_front() {
            Some(m) => Ok(m),
            None => {
                self.measure(ctx).await?;
                // SAFETY: measure fills the deque
                Ok(unsafe { self.humidity.pop_front_unchecked() })
            }
        }
    }

    async fn measure(&mut self, ctx: &mut ContextHandle<'_>) -> Result<(), Bme280Error<I2C>> {
        let measurement = {
            let _ = ctx.lock().await;
            if !self.initialized {
                self.init(ctx.delay_mut()).await?;
            }

            self.driver.measure(ctx.delay_mut()).await?
        };

        self.temperature
            .push_back(measurement.temperature.into())
            .map_err(|_| Bme280Error::<I2C>::FullBuffer("temperature"))?;
        self.pressure
            .push_back(measurement.pressure.into())
            .map_err(|_| Bme280Error::<I2C>::FullBuffer("pressure"))?;
        self.humidity
            .push_back(measurement.humidity.into())
            .map_err(|_| Bme280Error::<I2C>::FullBuffer("humidity"))?;
        Ok(())
    }
}

macro_rules! make_bme_sensor {
    ($name:ident) => {
        paste::paste! {
            #[derive(Debug)]
            pub struct [<Bme280 $name Sensor>]<'a, I2C: i2c::I2c, M: blocking_mutex::raw::RawMutex> {
                status: Status,
                device: &'a Mutex<M, Bme280Device<I2C>>,
            }

            impl<'a, I2C: i2c::I2c, M: blocking_mutex::raw::RawMutex> [<Bme280 $name Sensor>]<'a, I2C, M> {
                pub fn new(device: &'a Mutex<M, Bme280Device<I2C>>) -> Self {
                    Self {
                        status: Status::Initialized,
                        device,
                    }
                }
            }

            impl<'a, I2C: i2c::I2c + core::fmt::Debug, M: blocking_mutex::raw::RawMutex> Sensor<$name>
                for [<Bme280 $name Sensor>]<'a, I2C, M>
            {
                type Error = Bme280Error<I2C>;

                async fn read(&mut self, ctx: &mut ContextHandle<'_>) -> Result<$name, Self::Error> {
                    self.device
                        .lock()
                        .await
                        .[<get_ $name:lower _measurement>](ctx)
                        .await
                }
            }

            impl<'a, I2C: i2c::I2c + core::fmt::Debug, M: blocking_mutex::raw::RawMutex> Component
                for [<Bme280 $name Sensor>]<'a, I2C, M>
            {
                type Error = Bme280Error<I2C>;

                fn id(&self) -> DeviceId {
                    DeviceId::[<$name Sensor>]
                }

                fn status(&self) -> Status {
                    self.status
                }

                fn set_status(&mut self, status: Status) {
                    self.status = status;
                }

                fn run_once(
                    &mut self,
                    ctx: &mut ContextHandle<'_>,
                ) -> impl Future<Output = Result<(), Self::Error>> {
                    <Self as Sensor<_>>::run_once(self, ctx, self.id())
                }
            }
        }
    };
}

make_bme_sensor!(Temperature);
make_bme_sensor!(Pressure);
make_bme_sensor!(Humidity);
