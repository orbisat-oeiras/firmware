#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};
use orbisat::{Component, comms::PacketSink};
use orbisat_components::ConsoleByteSink;
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static CHANNEL: Channel<CriticalSectionRawMutex, Packet, 1> = Channel::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let sink = PacketSink::new(CHANNEL.receiver(), ConsoleByteSink);
    spawner.spawn(sink_task(sink)).unwrap();

    loop {
        CHANNEL
            .sender()
            .send(Packet::TmPacket(TmPacket::new(
                DeviceId::System,
                Timestamp::new(10).unwrap(),
                Payload::from_u8(10),
            )))
            .await;
        Timer::after_millis(500).await;
    }
}

#[embassy_executor::task]
async fn sink_task(mut sink: PacketSink<ConsoleByteSink>) {
    sink.run().await;
}
