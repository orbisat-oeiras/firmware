use orbipacket::Packet;
use tokio::sync::broadcast::{Receiver, error::RecvError};

pub struct ConsoleLogger {
    channel: Receiver<Packet>,
}

impl ConsoleLogger {
    pub fn new(channel: Receiver<Packet>) -> Self {
        Self { channel }
    }

    pub async fn steady(&mut self) -> anyhow::Result<()> {
        loop {
            match self.channel.recv().await {
                Ok(packet) => {
                    println!("Sending packet: {:?}", packet)
                }
                Err(RecvError::Closed) => {
                    break;
                }
                Err(RecvError::Lagged(skipped)) => {
                    println!(
                        "WARNING: ConsoleLogger has skipped {} packets due to broadcast channel lag.",
                        skipped
                    );
                }
            }
        }
    }
}
