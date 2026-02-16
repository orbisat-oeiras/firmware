use std::time::Duration;

use nmea::Nmea;
use orbipacket::{DeviceId, Packet, Payload};
use rppal::uart::Uart;
use tokio::sync::broadcast::Sender;

use crate::{cancellable, signal::SmartSignal, tmtc::TmPacketSender};

pub struct Gnss {
    uart: Uart,
    parser: Nmea,
    sender: TmPacketSender,
    buffer: String,
}

impl Gnss {
    pub fn new(uart: Uart, send: Sender<Packet>) -> Self {
        Self {
            uart,
            sender: TmPacketSender::new(send, DeviceId::GPS),
            parser: Nmea::default(),
            buffer: String::new(),
        }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        let mut carriage_return = false;
        let mut interval = tokio::time::interval(Duration::from_millis(250));

        cancellable!(cancel => {
            loop {
                while self.uart.input_len()? > 0 {
                    let mut buf = [0u8; 1];
                    if self.uart.read(&mut buf)? == 0 {
                        continue;
                    }

                    let c = buf[0] as char;

                    // print!("{}", c);

                    if c == '\r' {
                        carriage_return = true;
                    } else if c == '\n' && carriage_return {
                        let result = self.parser.parse(&self.buffer);
                        match result {
                            Ok(sentence) => {/*println!("Parsed {} sentence", sentence)*/},
                            Err(e) => println!("NMEA Error: {}", e),
                        }

                        self.buffer = "".to_string();
                        carriage_return = false;

                        let payload = (
                            self.parser.latitude.unwrap_or(f64::NAN).to_le_bytes(),
                            self.parser.longitude.unwrap_or(f64::NAN).to_le_bytes(),
                            self.parser.altitude.unwrap_or(f32::NAN).to_le_bytes()
                        );
                        let payload = unsafe {std::mem::transmute::<([u8; 8], [u8; 8], [u8; 4]), [u8; 20]>(payload)};

                        let payload = Payload::from_bytes(&payload[..])?;

                        self.sender.send(payload).await?;
                    } else if c != '\0' {
                        self.buffer.push(c);
                    }
                }
                interval.tick().await;
            }
        })
    }
}
