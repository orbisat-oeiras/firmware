use orbipacket::Packet;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SdRequest {
    WritePacket(Packet),
    LogMessage(heapless::String<128>),
    WriteWav(WavFile),
}

#[derive(Debug, thiserror::Error)]
pub enum WavError {
    #[error("provided buffer is too small to encode wav file")]
    BufferTooSmall,
}

#[derive(Debug)]
pub struct WavFile {
    num_channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    // TODO: the buffer size is arbitrary
    data: [u8; 8192],
}

impl WavFile {
    pub fn new(
        num_channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
        data: [u8; 8192],
    ) -> Self {
        Self {
            num_channels,
            sample_rate,
            bits_per_sample,
            data,
        }
    }

    pub fn to_bytes(&self, buf: &mut [u8]) -> Result<usize, WavError> {
        if buf.len() < 44 + self.data.len() {
            return Err(WavError::BufferTooSmall);
        }

        let mut idx = 0;

        buf[idx..idx + 4].copy_from_slice(b"RIFF");
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(&(44 + self.data.len() - 8).to_le_bytes());
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(b"WAVE");
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(b"fmt ");
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(&16u32.to_le_bytes());
        idx += 4;

        buf[idx..idx + 2].copy_from_slice(&1u16.to_le_bytes());
        idx += 2;

        buf[idx..idx + 2].copy_from_slice(&self.num_channels.to_le_bytes());
        idx += 2;

        buf[idx..idx + 4].copy_from_slice(&self.sample_rate.to_le_bytes());
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(&self.bytes_per_sec().to_le_bytes());
        idx += 4;

        buf[idx..idx + 2].copy_from_slice(&self.block_align().to_le_bytes());
        idx += 2;

        buf[idx..idx + 2].copy_from_slice(&self.bits_per_sample.to_le_bytes());
        idx += 2;

        buf[idx..idx + 4].copy_from_slice(b"data");
        idx += 4;

        buf[idx..idx + 4].copy_from_slice(&self.data.len().to_le_bytes());
        idx += 4;

        buf[idx..idx + self.data.len()].copy_from_slice(&self.data);
        idx += self.data.len();

        Ok(idx)
    }

    fn block_align(&self) -> u16 {
        self.num_channels * self.bits_per_sample / 8
    }

    fn bytes_per_sec(&self) -> u32 {
        self.sample_rate * (self.block_align() as u32)
    }
}
