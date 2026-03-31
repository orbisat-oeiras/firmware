#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::Async;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Config, UartRx, UartTx};
use esp_hal::{clock::CpuClock, uart::Uart};
use orbipacket::{DeviceId, Packet, Payload, Timestamp, TmPacket};
use orbisat::comms::PacketSource;
use orbisat::{Component, comms::PacketSink};
use orbisat_components::{ConsoleByteSink, SerialByteSink, SerialByteSource};
use orbisat_firmware_config::packet_channel::{
    InboundPacketChannel, InboundPacketChannelSubscriber, OutboundPacketChannel,
    OutboundPacketChannelPublisher,
};
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static OUTBOUND_PACKET_CHANNEL: OutboundPacketChannel = OutboundPacketChannel::new();
static INBOUND_PACKET_CHANNEL: InboundPacketChannel = InboundPacketChannel::new();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.0.1

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    let console_sink = PacketSink::new(
        OUTBOUND_PACKET_CHANNEL.subscriber().unwrap(),
        ConsoleByteSink,
    );

    let (uart_rx, uart_tx) = Uart::new(peripherals.UART2, Config::default().with_baudrate(19200))
        .unwrap()
        .with_rx(peripherals.GPIO12)
        .with_tx(peripherals.GPIO13)
        .into_async()
        .split();

    let serial_sink = PacketSink::new(
        OUTBOUND_PACKET_CHANNEL.subscriber().unwrap(),
        SerialByteSink::new(uart_tx),
    );

    let serial_source = PacketSource::new(
        INBOUND_PACKET_CHANNEL.publisher().unwrap(),
        SerialByteSource::new(uart_rx),
    );

    let mut tick = Ticker::every(Duration::from_millis(500));
    let mut counter = 0u32;
    let publisher = OUTBOUND_PACKET_CHANNEL.publisher().unwrap();
    let subscriber = INBOUND_PACKET_CHANNEL.subscriber().unwrap();

    spawner.spawn(console_sink_task(console_sink)).unwrap();
    spawner.spawn(serial_sink_task(serial_sink)).unwrap();
    spawner.spawn(serial_source_task(serial_source)).unwrap();
    spawner
        .spawn(inbound_packet_task(
            subscriber,
            OUTBOUND_PACKET_CHANNEL.publisher().unwrap(),
        ))
        .unwrap();

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
    sink.run().await.unwrap();
}

#[embassy_executor::task]
async fn serial_sink_task(mut sink: PacketSink<SerialByteSink<UartTx<'static, Async>>>) {
    sink.run().await.unwrap();
}

#[embassy_executor::task]
async fn serial_source_task(mut source: PacketSource<SerialByteSource<UartRx<'static, Async>>>) {
    source.run().await.unwrap();
}

#[embassy_executor::task]
async fn inbound_packet_task(
    mut subscriber: InboundPacketChannelSubscriber<'static>,
    publisher: OutboundPacketChannelPublisher<'static>,
) {
    loop {
        match subscriber.next_message().await {
            embassy_sync::pubsub::WaitResult::Lagged(_) => {}
            embassy_sync::pubsub::WaitResult::Message(packet) => match packet {
                Packet::TmPacket(_) => continue,
                Packet::TcPacket(packet) => {
                    if *packet.device_id() == DeviceId::TimeSync {
                        let t1 = Instant::now().as_micros();
                        let payload = packet.payload().as_bytes();

                        defmt::warn!("payload = {:02X}", payload);

                        let t0 = u64::from_le_bytes([
                            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5],
                            payload[6], payload[7],
                        ]);
                        let t2 = Instant::now().as_micros();

                        let payload = [t0.to_le_bytes(), t1.to_le_bytes(), t2.to_le_bytes()];
                        let payload: [u8; 3 * 8] = unsafe { core::mem::transmute(payload) };

                        publisher
                            .publish(Packet::TmPacket(TmPacket::new(
                                DeviceId::TimeSync,
                                Timestamp::new(10).unwrap(),
                                Payload::from_raw_bytes(payload).unwrap(),
                            )))
                            .await;
                    }
                }
            },
        }
    }
}
