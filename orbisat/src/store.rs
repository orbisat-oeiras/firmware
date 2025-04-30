use std::{fs::File, io::Write as _, path::PathBuf};

use orbipacket::Packet;
use tokio::sync::broadcast::{error::RecvError, Receiver};

use crate::{cancellable, signal::SmartSignal};

pub struct FileStore {
    file: File,
    channel: Receiver<Packet>,
    buffer: [u8; Packet::encode_buffer_size()],
}

impl FileStore {
    pub fn new(channel: Receiver<Packet>) -> anyhow::Result<Self> {
        let filename = format!(
            "{}",
            chrono::Utc::now().format("TMPACKETS-%F-%H-%M-%S-&.9f%z.log")
        );
        let mut path = PathBuf::new();
        path.push(filename);
        let file = File::create(path)?;
        println!("here");
        Ok(Self {
            file,
            channel,
            buffer: [0u8; Packet::encode_buffer_size()],
        })
    }

    pub async fn steady(&mut self, cancel: SmartSignal) -> anyhow::Result<()> {
        cancellable!(cancel => {
            loop {
                match self.channel.recv().await {
                    Ok(packet) => {self.file.write_all(packet.encode(&mut self.buffer[..])?)?;},
                    Err(RecvError::Closed) => {break;},
                    Err(RecvError::Lagged(skipped)) => {
                        println!("WARNING: FileStore has skipped {} packets due to broadcast channel lag.", skipped);
                    }
                }
            }
        })
    }
}
