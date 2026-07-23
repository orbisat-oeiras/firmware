use orbipacket::Packet;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SdRequest {
    WritePacket(Packet),
    LogMessage(heapless::String<128>),
}
