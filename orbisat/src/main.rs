use embedded_hal_bus::{i2c::AtomicDevice, util::AtomicCell};
use orbisat::{
    sensor::PressureTemperatureHumiditySensor, signal::SmartSignal, store::FileStore,
    tmtc::SerialPacketSink,
};
use rppal::{hal, i2c, uart};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let signal = SmartSignal::new()?;

    //let uart = uart::Uart::new(19200, uart::Parity::None, 8, 1)?;
    let uart = uart::Uart::with_path("/dev/ttyUSB0", 19200, uart::Parity::None, 8, 1)?;
    let (tx, rx) = tokio::sync::broadcast::channel(100);

    let mut sink = SerialPacketSink::new(uart, rx);

    let mut store = FileStore::new(tx.subscribe())?;

    let i2c = i2c::I2c::new()?;
    let i2c = AtomicCell::new(i2c);

    let delay = hal::Delay::new();

    let sensor = PressureTemperatureHumiditySensor::new(AtomicDevice::new(&i2c), delay, tx.clone());
    let mut sensor = match sensor {
        Ok(sensor) => sensor,
        Err(err) => anyhow::bail!(format!("{:?}", err)),
    };

    tokio::try_join!(
        sink.steady(signal.clone()),
        sensor.steady(signal.clone()),
        store.steady(signal.clone()),
    )?;

    Ok(())
}
