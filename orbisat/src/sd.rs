use orbipacket::Packet;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SdRequest<const N: usize> {
    WritePacket(Packet),
    LogMessage(heapless::String<N>),
}
