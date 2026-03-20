#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use esp_hal::Async;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::Config;
use esp_hal::{clock::CpuClock, uart::Uart};
use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};
use orbisat::{Component, comms::PacketSink};
use orbisat_components::{ConsoleByteSink, SerialByteSink};
use orbisat_firmware_config::packet_channel::PacketChannel;
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static CHANNEL: PacketChannel = PacketChannel::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let console_sink = PacketSink::new(CHANNEL.subscriber().unwrap(), ConsoleByteSink);

    let serial_sink = PacketSink::new(
        CHANNEL.subscriber().unwrap(),
        SerialByteSink::new(
            Uart::new(peripherals.UART2, Config::default().with_baudrate(19200))
                .unwrap()
                .with_rx(peripherals.GPIO12)
                .with_tx(peripherals.GPIO13)
                .into_async(),
        ),
    );

    spawner.spawn(console_sink_task(console_sink)).unwrap();
    spawner.spawn(serial_sink_task(serial_sink)).unwrap();

    let mut tick = Ticker::every(Duration::from_millis(500));
    let mut counter = 0u32;
    let publisher = CHANNEL.publisher().unwrap();

    loop {
        publisher
            .publish(Packet::TmPacket(TmPacket::new(
                DeviceId::System,
                Timestamp::new(10).unwrap(),
                Payload::from_u32(counter),
            )))
            .await;

        counter += 1;
        tick.next().await;
    }
}

#[embassy_executor::task]
async fn console_sink_task(mut sink: PacketSink<ConsoleByteSink>) {
    sink.run().await;
}

#[embassy_executor::task]
async fn serial_sink_task(mut sink: PacketSink<SerialByteSink<Uart<'static, Async>>>) {
    sink.run().await;
}
