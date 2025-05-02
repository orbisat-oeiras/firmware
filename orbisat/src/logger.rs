use orbipacket::Packet;
use tokio::sync::broadcast::{error::RecvError, Receiver};

use crate::{cancellable, signal::SmartSignal};

pub struct ConsoleLogger {
    channel: Receiver<Packet>,
}

impl ConsoleLogger {
    pub fn new(channel: Receiver<Packet>) -> Self {
        Self { channel }
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        cancellable!(cancel => {
            loop {
                match self.channel.recv().await {
                    Ok(packet) => {println!("Sending packet: {:?}", packet)},
                    Err(RecvError::Closed) => {break;},
                    Err(RecvError::Lagged(skipped)) => {
                        println!("WARNING: ConsoleLogger has skipped {} packets due to broadcast channel lag.", skipped);
                    }
                }
            }
        })
    }
}
