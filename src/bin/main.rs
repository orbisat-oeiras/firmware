use embedded_hal_bus::{i2c::AtomicDevice, util::AtomicCell};
use orbisat::{
    logger::ConsoleLogger,
    navigation::Gnss,
    sensor::PressureTemperatureHumiditySensor,
    signal::SmartSignal,
    store::FileStore,
    system::{AltitudeMonitor, HeartbeatSender},
    tmtc::SerialPacketSink,
};
use rppal::{hal, i2c, uart};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let signal = SmartSignal::new()?;

    let uart = uart::Uart::with_path("/dev/ttyUSB0", 19200, uart::Parity::None, 8, 1)?;
    let (tx, rx) = tokio::sync::broadcast::channel(100);

    let mut sink = SerialPacketSink::new(uart, rx);

    let mut store = FileStore::new(tx.subscribe())?;

    let mut logger = ConsoleLogger::new(tx.subscribe());

    let mut heartbeat = HeartbeatSender::new(tx.clone());

    let i2c = i2c::I2c::new()?;
    let i2c = AtomicCell::new(i2c);

    let delay = hal::Delay::new();

    let sensor = PressureTemperatureHumiditySensor::new(AtomicDevice::new(&i2c), delay, tx.clone());
    let mut sensor = match sensor {
        Ok(sensor) => sensor,
        Err(err) => anyhow::bail!(format!("{:?}", err)),
    };

    let uart = uart::Uart::new(9600, uart::Parity::None, 8, 1)?;
    let mut gnss = Gnss::new(uart, tx.clone());

    let mut monitor = AltitudeMonitor::new(tx.subscribe(), tx.clone());

    tokio::try_join!(
        heartbeat.steady(signal.clone()),
        sink.steady(signal.clone()),
        store.steady(signal.clone()),
        logger.steady(signal.clone()),
        // monitor.steady(signal.clone()),
        sensor.steady(signal.clone()),
        gnss.steady(signal.clone()),
    )?;

    Ok(())
}
