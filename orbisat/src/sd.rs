use orbipacket::Packet;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SdRequest {
    WritePacket(Packet),
    LogMessage(heapless::String<128>),
}

pub const WAV_HEADER_SIZE: usize = 44;

pub fn wav_header(
    num_channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    data_size: u32,
) -> [u8; WAV_HEADER_SIZE] {
    let block_align = num_channels * bits_per_sample / 8;
    let bytes_per_sec = sample_rate * u32::from(block_align);
    let mut header = [0; WAV_HEADER_SIZE];

    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36 + data_size).to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes());
    header[20..22].copy_from_slice(&1u16.to_le_bytes());
    header[22..24].copy_from_slice(&num_channels.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&bytes_per_sec.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_size.to_le_bytes());

    header
}
