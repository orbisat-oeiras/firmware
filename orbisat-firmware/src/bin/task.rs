#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Delay, Duration, Ticker};
use esp_hal::Async;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, UartRx, UartTx};
use esp_hal::{clock::CpuClock, uart::Uart};
use orbipacket::{DeviceId, Payload};
use orbisat::comms::PacketSource;
use orbisat::{Component, comms::PacketSink};
use orbisat::{Context, ContextHandle};
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
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let ctx = CONTEXT.init(Context::new(
        InboundPacketChannel::new(),
        OutboundPacketChannel::new(),
        Delay,
    ));

    #[cfg(feature = "esp32")]
    let (uart_rx, uart_tx) = Uart::new(peripherals.UART2, Config::default().with_baudrate(19200))
        .unwrap()
        .with_rx(peripherals.GPIO12)
        .with_tx(peripherals.GPIO13)
        .into_async()
        .split();

    #[cfg(feature = "esp32s3")]
    let (uart_rx, uart_tx) = Uart::new(peripherals.UART2, Config::default().with_baudrate(19200))
        .unwrap()
        .with_rx(peripherals.GPIO1)
        .with_tx(peripherals.GPIO2)
        .into_async()
        .split();

    let mut tick = Ticker::every(Duration::from_millis(500));
    let mut counter = 0u32;
    let ctx_handle = ctx.to_handle().unwrap();

    components! {
        (spawner, ctx) {
            console_sink: PacketSink<ConsoleByteSink> = (ctx.outbound().subscriber().unwrap(), ConsoleByteSink);
            serial_sink: PacketSink<SerialByteSink<UartTx<'static, Async>>> = (
                ctx.outbound().subscriber().unwrap(), SerialByteSink::new(uart_tx),
            );
            serial_source: PacketSource<SerialByteSource<UartRx<'static, Async>>> = (
                ctx.inbound().publisher().unwrap(),
                SerialByteSource::new(uart_rx),
            );
            time_sync: TimeSyncComponent = ();
        }
    }

    info!("Components initialized");

    loop {
        ctx_handle
            .send_outbound(DeviceId::System, Payload::from_u32(counter))
            .unwrap()
            .await;

        counter += 1;
        tick.next().await;
    }
}
