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
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Async, Blocking,
    clock::CpuClock,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    ledc::timer::config::Duty,
    timer::timg::TimerGroup,
    uart::{Config as UartConfig, Uart, UartRx, UartTx},
};
#[cfg(feature = "esp32s3")]
use esp_hal::{
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};
use orbisat::{
    Context,
    comms::{PacketSink, PacketSource},
};
use orbisat_components::{
    ConsoleByteSink, SerialByteSink, SerialByteSource, TimeSyncComponent,
    primary::{Bme280Device, Bme280HumiditySensor, Bme280PressureSensor, Bme280TemperatureSensor},
    sd::SdCardManager,
    secondary::SpeakerComponent,
    spatial::Mma8542Component,
};
use orbisat_firmware::{components, pwm::PwmController, sweep};
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

    // let mut usb = UsbSerialJtag::new(peripherals.USB_DEVICE);
    // let _ = writeln!(usb, "USB alive");
    // embassy_time::Timer::after(Duration::from_millis(500)).await;

    info!("Embassy initialized!");

    // GET PERIPHERALS

    // Uart for radio comms
    #[cfg(feature = "esp32")]
    let (uart_rx, uart_tx) = Uart::new(
        peripherals.UART2,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO16)
    .with_tx(peripherals.GPIO17)
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

    info!("Initialised peripherals (1/5): UART");

    // I2c for the sensor
    let i2c0 = I2c::new(peripherals.I2C0, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO21)
        .with_sda(peripherals.GPIO18)
        .into_async();

    info!("Initialised peripherals (2/5): I2C0");

    // I2c for the accelerometer
    #[cfg(feature = "esp32")]
    let i2c1 = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO5)
        .with_sda(peripherals.GPIO18);

    #[cfg(feature = "esp32s3")]
    let i2c1 = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO8)
        .with_sda(peripherals.GPIO9);

    info!("Initialised peripherals (3/5): I2C1");

    // Pwm for audio output

    #[cfg(feature = "esp32")]
    let pwm = PwmController::new(peripherals.LEDC, peripherals.GPIO33, Duty::Duty10Bit);
    #[cfg(feature = "esp32s3")]
    let pwm = PwmController::new(peripherals.LEDC, peripherals.GPIO34, Duty::Duty10Bit);

    info!("Initialised peripherals (4/5): LEDC");

    // Spi for SD card

    #[cfg(feature = "esp32s3")]
    let spi_bus = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_khz(400))
            .with_mode(SpiMode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO12)
    .with_miso(peripherals.GPIO13)
    .with_mosi(peripherals.GPIO11)
    .into_async();

    let spi_cs = Output::new(peripherals.GPIO10, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi_bus, spi_cs, Delay)
        .expect("should be able to create an ExclusiveDevice");

    info!("Initialised peripherals (5/5): SPI");

    let _sd_manager = SdCardManager::new(spi_dev);

    // Sensor device
    let bme = Bme280Device::new(i2c0);

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
            accelerometer: Mma8542Component<I2c<'static, Blocking>> = (i2c1).expect("should be able to create Mma8542Component");
            speaker: SpeakerComponent<'static, PwmController> = (pwm, &sweep::SWEEP[..]);
        }
    }

    info!("Components initialized");
}
