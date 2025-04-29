use std::time::Duration;

use bme280::{i2c::BME280, Configuration, IIRFilter, Oversampling};
use embedded_hal::{delay::DelayNs, i2c};
use mma8x5x::{Mma8x5x, ModeChangeError};
use orbipacket::{DeviceId, Packet, Payload};
use tokio::sync::broadcast::Sender;

use crate::{cancellable, signal::SmartSignal, tmtc::TmPacketSender};

pub struct PressureTemperatureHumiditySensor<D, I2C>
where
    D: DelayNs,
    I2C: i2c::I2c + i2c::ErrorType,
{
    bme: BME280<I2C>,
    pressure_sender: TmPacketSender,
    temperature_sender: TmPacketSender,
    humidity_sender: TmPacketSender,
    delay: D,
}

impl<D, I2C> PressureTemperatureHumiditySensor<D, I2C>
where
    D: DelayNs,
    I2C: i2c::I2c + i2c::ErrorType,
{
    pub fn new(
        i2c: I2C,
        mut delay: D,
        send: Sender<Packet>,
    ) -> Result<Self, bme280::Error<I2C::Error>> {
        let mut bme = BME280::new_primary(i2c);
        bme.init_with_config(
            &mut delay,
            Configuration::default()
                .with_pressure_oversampling(Oversampling::Oversampling4X)
                .with_temperature_oversampling(Oversampling::Oversampling4X)
                .with_humidity_oversampling(Oversampling::Oversampling4X)
                .with_iir_filter(IIRFilter::Coefficient16),
        )?;
        Ok(Self {
            bme,
            pressure_sender: TmPacketSender::new(send.clone(), DeviceId::PressureSensor),
            temperature_sender: TmPacketSender::new(send.clone(), DeviceId::TemperatureSensor),
            humidity_sender: TmPacketSender::new(send, DeviceId::HumiditySensor),
            delay,
        })
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        cancellable!(cancel => {
            loop {
                let measurement = self.bme.measure(&mut self.delay);
                let measurement = match measurement {
                    Ok(measurement) => measurement,
                    Err(err) => {
                        anyhow::bail!(format!("{:?}", err))
                    }
                };

                let packet = Payload::from_bytes(&measurement.pressure.to_le_bytes()[..])?;
                //println!("Pressure");
                self.pressure_sender.send(packet).await?;
                let packet = Payload::from_bytes(&measurement.temperature.to_le_bytes()[..])?;
                //println!("Temperature");
                self.temperature_sender.send(packet).await?;
                let packet = Payload::from_bytes(&measurement.humidity.to_le_bytes()[..])?;
                //println!("Humidity");
                self.humidity_sender.send(packet).await?;

                interval.tick().await;
            }
        })
    }
}

pub struct Accelerometer<I2C, E>
where
    I2C: i2c::I2c<i2c::SevenBitAddress, Error = E>,
    E: std::fmt::Debug,
{
    mma: Mma8x5x<I2C, mma8x5x::ic::Mma8452, mma8x5x::mode::Active>,
    packet_sender: TmPacketSender,
}

impl<I2C, E> Accelerometer<I2C, E>
where
    I2C: i2c::I2c<i2c::SevenBitAddress, Error = E>,
    E: std::fmt::Debug,
{
    pub fn new(
        i2c: I2C,
        send: Sender<Packet>,
    ) -> Result<Self, ModeChangeError<E, Mma8x5x<I2C, mma8x5x::ic::Mma8452, mma8x5x::mode::Standby>>>
    {
        let mma = Mma8x5x::new_mma8452(i2c, mma8x5x::SlaveAddr::Default);
        Ok(Self {
            mma: mma.into_active()?,
            packet_sender: TmPacketSender::new(send, DeviceId::Accelerometer),
        })
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        cancellable!(cancel => {
            loop {
                let measurement = self.mma.read();
                match measurement {
                    Ok(measurement) => {
                        let mut packet = [0u8; 12];
                        packet[..4].copy_from_slice(&measurement.x.to_le_bytes()[..]);
                        packet[4..8].copy_from_slice(&measurement.y.to_le_bytes()[..]);
                        packet[8..12].copy_from_slice(&measurement.z.to_le_bytes()[..]);
                        let packet = Payload::from_bytes(&packet)?;

                        self.packet_sender.send(packet).await?;
                        interval.tick().await;
                    },
                    Err(err) => {
                        match err {
                            mma8x5x::Error::I2C(e) => anyhow::bail!(format!("{:?}", e))
                        }
                    }
                }
            }
        })
    }
}
