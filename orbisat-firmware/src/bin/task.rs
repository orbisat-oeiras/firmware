#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![allow(clippy::type_complexity)]

use core::sync::atomic::AtomicU8;
#[cfg(feature = "esp32s3")]
use core::sync::atomic::Ordering;

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration};
#[cfg(feature = "esp32s3")]
use embedded_hal_bus::spi::ExclusiveDevice;
#[cfg(feature = "esp32s3")]
use embedded_sdmmc::{Directory, File, Mode, SdCard, Volume, VolumeIdx};
#[cfg(feature = "esp32s3")]
use esp_hal::gpio::Output;
use esp_hal::{
    Async, Blocking,
    clock::CpuClock,
    i2c::master::I2c,
    uart::{UartRx, UartTx},
};
#[cfg(feature = "esp32s3")]
use esp_hal::{spi::master::Spi, system::Stack};
#[cfg(feature = "esp32s3")]
use esp_rtos::embassy::Executor;
use orbisat::{
    channels::{InboundPacketChannel, OutboundPacketChannel, SdRequestChannel},
    comms::{PacketSink, PacketSource},
    context::Context,
};
use orbisat_components::{
    ConsoleByteSink, SerialByteSink, SerialByteSource,
    primary::{Bme280Device, Bme280HumiditySensor, Bme280PressureSensor, Bme280TemperatureSensor},
    spatial::Mma8542Component,
};
#[cfg(feature = "esp32s3")]
use orbisat_components::{
    TimeSyncComponent,
    sd::{SdCardManager, SdFileWriter, SdTimeSource},
};
#[cfg(feature = "esp32s3")]
use orbisat_firmware::peripherals::second_core::SecondCorePeripheralManager;
use orbisat_firmware::{components, peripherals::PeripheralManager};
use static_cell::StaticCell;
use {esp_backtrace as _, esp_println as _};
// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static BOOTCOUNT: AtomicU8 = AtomicU8::new(0);

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // generator version: 1.0.1

    // INITIALIZE EMBASSY

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // GET PERIPHERALS
    let mut p = PeripheralManager::new(peripherals);

    // START THE SCHEDULER
    let timg0 = p.take_timg0().unwrap();
    let sw_ints = p.take_software_interrupts().unwrap();
    esp_rtos::start(timg0.timer0, sw_ints.software_interrupt0);

    info!("Embassy initialized!");

    // CREATE CONTEXT
    static CONTEXT: StaticCell<Context> = StaticCell::new();
    let ctx = CONTEXT.init(Context::new(
        InboundPacketChannel::new(),
        OutboundPacketChannel::new(),
        SdRequestChannel::new(),
        Duration::from_millis(500),
        Delay,
    ));

    // SPLIT RADIO UART
    let (uart0_rx, uart0_tx) = p.take_uart0().unwrap().split();

    // BME DRIVER
    let bme = Bme280Device::new(p.take_i2c0().unwrap());

    static BME_MUTEX: StaticCell<Mutex<CriticalSectionRawMutex, Bme280Device<I2c<'_, Async>>>> =
        StaticCell::new();
    let bme_mutex = BME_MUTEX.init(Mutex::new(bme));

    bme_mutex
        .get_mut()
        .init(&mut Delay)
        .await
        .expect("should be able to initialize BME280 driver");

    // SPAWN COMPONENT TASKS
    components! {
        (spawner, ctx) {
            console_sink: PacketSink<ConsoleByteSink> = (
                ctx.outbound().subscriber().expect("outbound should be subscribable"),
                ConsoleByteSink,
            );
            serial_sink: PacketSink<SerialByteSink<UartTx<'static, Async>>> = (
                ctx.outbound().subscriber().expect("outbound should be subscribable"),
                SerialByteSink::new(uart0_tx),
            );
            serial_source: PacketSource<SerialByteSource<UartRx<'static, Async>>> = (
                ctx.inbound().publisher().expect("inbound should be publishable"),
                SerialByteSource::new(uart0_rx),
            );
            time_sync: TimeSyncComponent<'static> = (&BOOTCOUNT);
            temperature_sensor: Bme280TemperatureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            pressure_sensor: Bme280PressureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            humidity_sensor: Bme280HumiditySensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            accelerometer: Mma8542Component<I2c<'static, Blocking>> = (p.take_i2c1().unwrap()).expect("should be able to create Mma8542Component");
            // gnss: GnssComponent<UartRx<'static, Async>> = (uart1_rx);
        }
    }

    info!("Components initialized");

    // START SECOND CORE

    #[cfg(feature = "esp32s3")]
    {
        // TODO: the size of this stack is completely arbitrary
        static CORE1_STACK: StaticCell<Stack<8192>> = StaticCell::new();
        let core1_stack = CORE1_STACK.init(Stack::new());

        let second_core = p.take_second_core().unwrap();

        esp_rtos::start_second_core(
            p.take_cpu_control().unwrap(),
            sw_ints.software_interrupt1,
            core1_stack,
            move || {
                static EXECUTOR: StaticCell<Executor> = StaticCell::new();
                let executor = EXECUTOR.init(Executor::new());

                executor.run(|spawner| core1_main(spawner, second_core));
            },
        );
    }
}

#[cfg(feature = "esp32s3")]
fn core1_main(_spawner: Spawner, mut p: SecondCorePeripheralManager) {
    // SETUP SD CARD
    let (bootcount, data_file, timestamps_writer, _audio_writer) = {
        // Spi for SD card
        let spi_dev = ExclusiveDevice::new(
            p.take_spi().unwrap().into_async(),
            p.take_spi_cs().unwrap(),
            Delay,
        )
        .expect("should be able to create an ExclusiveDevice");

        info!("Initialised peripherals (6/6): SPI");

        static SD_CARD_MANAGER: StaticCell<
            SdCardManager<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>,
        > = StaticCell::new();

        let sd_card_manager = SD_CARD_MANAGER.init(match SdCardManager::new(spi_dev) {
            Ok(m) => m,
            Err(e) => match e {
                orbisat_components::sd::SdError::Sd(error) => panic!("sd error: {:?}", error),
                orbisat_components::sd::SdError::BootcountUnreadable => {
                    panic!("bootcount unreadable")
                }
            },
        });

        static VOLUME: StaticCell<
            Volume<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();
        static ROOT_DIR: StaticCell<
            Directory<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();
        static BOOT_DIR: StaticCell<
            Directory<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();
        static DATA_FILE: StaticCell<
            File<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();
        static AUDIO_FILE: StaticCell<
            File<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();
        static TIMESTAMPS_FILE: StaticCell<
            File<
                '_,
                SdCard<ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>, Delay>,
                SdTimeSource,
                4,
                4,
                1,
            >,
        > = StaticCell::new();

        let volume = VOLUME.init(
            sd_card_manager
                .volume_manager()
                .open_volume(VolumeIdx(0))
                .expect("should be able to open volume"),
        );
        let root_dir = ROOT_DIR.init(
            volume
                .open_root_dir()
                .expect("should be able to open root dir"),
        );
        let boot_dir = BOOT_DIR.init(
            root_dir
                .open_dir(sd_card_manager.boot_dir_name())
                .expect("should be able to open boot dir"),
        );

        let data_file = DATA_FILE.init(
            boot_dir
                .open_file_in_dir("DATA", Mode::ReadWriteCreateOrTruncate)
                .expect("should be able to open data file"),
        );
        let audio_file = AUDIO_FILE.init(
            boot_dir
                .open_file_in_dir("AUDIO", Mode::ReadWriteCreateOrTruncate)
                .expect("should be able to open audio file"),
        );
        let timestamps_file = TIMESTAMPS_FILE.init(
            boot_dir
                .open_file_in_dir("TIME", Mode::ReadWriteCreateOrTruncate)
                .expect("should be able to open audio file"),
        );

        let audio_writer = SdFileWriter::new(audio_file);
        let timestamps_writer = SdFileWriter::new(timestamps_file);

        (
            sd_card_manager.bootcount(),
            data_file,
            timestamps_writer,
            audio_writer,
        )
    };

    BOOTCOUNT.store(bootcount, Ordering::Relaxed);
    defmt::info!("Stored global bootcount");
}
