#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration};
use esp_hal::Async;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config as UartConfig, UartRx, UartTx};
use esp_hal::{clock::CpuClock, uart::Uart};
use orbisat::comms::PacketSource;
use orbisat::{Component, comms::PacketSink};
use orbisat::{Context, ContextHandle};
use orbisat_components::primary::{
    Bme280Device, Bme280HumiditySensor, Bme280PressureSensor, Bme280TemperatureSensor,
};
use orbisat_components::{ConsoleByteSink, SerialByteSink, SerialByteSource, TimeSyncComponent};
use orbisat_firmware::components;
use orbisat_firmware_config::packet_channel::{InboundPacketChannel, OutboundPacketChannel};
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static CONTEXT: StaticCell<Context> = StaticCell::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // generator version: 1.0.1

    // INITIALIZE EMBASSY

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // CONFIGURE PERIPHERALS

    #[cfg(feature = "esp32")]
    let (uart_rx, uart_tx) = Uart::new(
        peripherals.UART2,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO12)
    .with_tx(peripherals.GPIO13)
    .into_async()
    .split();

    #[cfg(feature = "esp32s3")]
    let (uart_rx, uart_tx) = Uart::new(
        peripherals.UART2,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO1)
    .with_tx(peripherals.GPIO2)
    .into_async()
    .split();

    let i2c = I2c::new(peripherals.I2C0, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO21)
        .with_sda(peripherals.GPIO19)
        .into_async();

    let bme = Bme280Device::new(i2c);

    static BME_MUTEX: StaticCell<Mutex<CriticalSectionRawMutex, Bme280Device<I2c<'_, Async>>>> =
        StaticCell::new();
    let bme_mutex = BME_MUTEX.init(Mutex::new(bme));

    // CREATE CONTEXT

    let ctx = CONTEXT.init(Context::new(
        InboundPacketChannel::new(),
        OutboundPacketChannel::new(),
        Duration::from_millis(500),
        Delay,
    ));

    // SPAWN COMPONENT TASKS

    components! {
        (spawner, ctx) {
            console_sink: PacketSink<ConsoleByteSink> = (
                ctx.outbound().subscriber().expect("outbound should be subscribable"),
                ConsoleByteSink,
            );
            serial_sink: PacketSink<SerialByteSink<UartTx<'static, Async>>> = (
                ctx.outbound().subscriber().expect("outbound should be subscribable"),
                SerialByteSink::new(uart_tx),
            );
            serial_source: PacketSource<SerialByteSource<UartRx<'static, Async>>> = (
                ctx.inbound().publisher().expect("inbound should be publishable"),
                SerialByteSource::new(uart_rx),
            );
            time_sync: TimeSyncComponent = ();
            temperature_sensor: Bme280TemperatureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            pressure_sensor: Bme280PressureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            humidity_sensor: Bme280HumiditySensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
        }
    }

    info!("Components initialized");
}
