#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![allow(clippy::type_complexity)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{Directory, File, Mode, SdCard, Volume, VolumeIdx};
use esp_hal::{
    Async, Blocking,
    clock::CpuClock,
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    i2s::master::{Channels, Config as I2sConfig, DataFormat, I2s, I2sRx},
    ledc::timer::config::Duty,
    peripherals::TIMG0,
    timer::timg::{TimerGroup, Wdt},
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
use orbisat_components::sd::SdTimeSource;
use orbisat_components::{
    ConsoleByteSink, SerialByteSink, SerialByteSource, TimeSyncComponent,
    primary::{Bme280Device, Bme280HumiditySensor, Bme280PressureSensor, Bme280TemperatureSensor},
    sd::{SdCardManager, SdFileWriter},
    secondary::SpeakerComponent,
    spatial::{GnssComponent, Mma8542Component},
};
use orbisat_firmware::i2s::AudioRecorderComponent;
use orbisat_firmware::{components, pwm::PwmController, sweep};
use orbisat_firmware_config::packet_channel::{
    AsyncMutex, InboundPacketChannel, OutboundPacketChannel,
};
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
    let (uart0_rx, uart0_tx) = Uart::new(
        peripherals.UART2,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO16)
    .with_tx(peripherals.GPIO17)
    .into_async()
    .split();

    #[cfg(feature = "esp32s3")]
    let (uart0_rx, uart0_tx) = Uart::new(
        peripherals.UART2,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO2)
    .with_tx(peripherals.GPIO1)
    .into_async()
    .split();

    info!("Initialised peripherals (1/6): UART0");

    let (uart1_rx, _) = Uart::new(
        peripherals.UART1,
        UartConfig::default().with_baudrate(19200),
    )
    .expect("should be able to construct a Uart")
    .with_rx(peripherals.GPIO3)
    .with_tx(peripherals.GPIO5)
    .into_async()
    .split();

    info!("Initialised peripherals (2/6): UART1");

    // I2c for the sensor
    #[cfg(feature = "esp32")]
    let i2c0 = I2c::new(peripherals.I2C0, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO21)
        .with_sda(peripherals.GPIO22)
        .into_async();

    #[cfg(feature = "esp32s3")]
    let i2c0 = I2c::new(peripherals.I2C0, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO21)
        .with_sda(peripherals.GPIO42)
        .into_async();

    info!("Initialised peripherals (3/6): I2C0");

    // I2c for the accelerometer
    #[cfg(feature = "esp32")]
    let i2c1 = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO19)
        .with_sda(peripherals.GPIO18);

    #[cfg(feature = "esp32s3")]
    let i2c1 = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("should be able to construct an I2c")
        .with_scl(peripherals.GPIO8)
        .with_sda(peripherals.GPIO9);

    info!("Initialised peripherals (4/6): I2C1");

    // Pwm for audio output

    #[cfg(feature = "esp32")]
    let pwm = PwmController::new(peripherals.LEDC, peripherals.GPIO33, Duty::Duty10Bit);
    #[cfg(feature = "esp32s3")]
    let pwm = PwmController::new(peripherals.LEDC, peripherals.GPIO34, Duty::Duty10Bit);

    info!("Initialised peripherals (5/6): LEDC");

    // Spi for SD card

    #[cfg(feature = "esp32s3")]
    let (data_file, timestamps_writer, audio_writer) = {
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

        (data_file, timestamps_writer, audio_writer)
    };

    // I2S setup for audio recording
    // let (rx_buffer, rx_descriptors, _, _) = dma_buffers!(8 * 1024, 0);

    // let i2s = I2s::new(
    //     peripherals.I2S0,
    //     peripherals.DMA_CH0,
    //     I2sConfig::new_tdm_philips()
    //         .with_sample_rate(Rate::from_hz(6000))
    //         .with_data_format(DataFormat::Data16Channel16)
    //         .with_channels(Channels::STEREO),
    // )
    // .unwrap();
    // let i2s = i2s.with_mclk(peripherals.GPIO39);

    // static I2S_RX: StaticCell<I2sRx<'_, Blocking>> = StaticCell::new();

    // let i2s_rx = I2S_RX.init(
    //     i2s.i2s_rx
    //         .with_bclk(peripherals.GPIO37)
    //         .with_ws(peripherals.GPIO36)
    //         .with_din(peripherals.GPIO18)
    //         .build(rx_descriptors),
    // );

    // let transfer: esp_hal::dma::DmaTransferRxCircular<'_, I2sRx<'_, Blocking>> =
    //     i2s_rx.read_dma_circular(rx_buffer).unwrap();

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
        AsyncMutex::new(()),
    ));

    // INITIALIZE BME DRIVER
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
            time_sync: TimeSyncComponent = ();
            temperature_sensor: Bme280TemperatureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            pressure_sensor: Bme280PressureSensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            humidity_sensor: Bme280HumiditySensor<'static, I2c<'static, Async>, CriticalSectionRawMutex>  = (bme_mutex);
            accelerometer: Mma8542Component<I2c<'static, Blocking>> = (i2c1).expect("should be able to create Mma8542Component");
            // gnss: GnssComponent<UartRx<'static, Async>> = (uart1_rx);
        }
    }

    #[cfg(feature = "esp32s3")]
    components! {
        (spawner, ctx){
            sd_sink: PacketSink<SdFileWriter<'static, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>> = (
                ctx.outbound().subscriber().expect("outbound should be subscribable"),
                SdFileWriter::new(data_file));
            // speaker: SpeakerComponent<'static, PwmController<'static>, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>> = (pwm, &sweep::SWEEP[..], timestamps_writer);
            // audio_recorder: AudioRecorderComponent<'static> = (transfer, audio_writer, wdt);
        }
    }

    // let ctx_handle = ctx
    //     .to_handle()
    //     .expect("should be able to get context handle");

    info!("Components initialized");

    // loop {
    //     // let mut buf = [0; 4 * 1024];
    //     {
    //         let mut transfer = i2s_rx
    //             .read_dma(rx_buffer)
    //             .expect("should be able to read dma");
    //         while !transfer.is_done() {
    //             Timer::after(Duration::from_millis(100)).await;
    //         }
    //         defmt::info!("Transfer done");
    //     }

    //     audio_writer
    //         .write(rx_buffer, &ctx_handle)
    //         .await
    //         .expect("should be able to write audio");
    //     defmt::info!("Wrote audio");
    //     embassy_futures::yield_now().await;
    // }
}
