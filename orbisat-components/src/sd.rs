use core::fmt::Debug;

use embassy_time::{Delay, Instant};
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{
    BlockDevice, File, Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager,
};
use heapless::{String, format};
use orbipacket::{DeviceId, Packet};
use orbisat::{
    Component, Status,
    channels::SdRequestChannelReceiver,
    comms::{ByteSink, CommunicationError},
    sd::SdRequest,
};

#[derive(Debug)]
pub struct SdTimeSource;

impl TimeSource for SdTimeSource {
    fn get_timestamp(&self) -> embedded_sdmmc::Timestamp {
        let time = Instant::now().as_micros().to_be_bytes();

        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: time[0],
            hours: time[1],
            minutes: time[2],
            seconds: time[3],
        }
    }
}

#[derive(thiserror::Error)]
pub enum SdError<SPI: SpiDevice<u8>> {
    #[error("sd error: {0:?}")]
    Sd(embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>),
    #[error("bootcount file cannot be read")]
    BootcountUnreadable,
    #[error(transparent)]
    Communication(#[from] orbisat::comms::CommunicationError),
}

impl<SPI: SpiDevice<u8>> Debug for SdError<SPI> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sd(arg0) => f.debug_tuple("Sd").field(arg0).finish(),
            Self::BootcountUnreadable => write!(f, "BootcountUnreadable"),
            Self::Communication(comm) => write!(f, "{:?}", comm),
        }
    }
}

impl<SPI: SpiDevice<u8>> From<embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>>
    for SdError<SPI>
{
    fn from(value: embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>) -> Self {
        Self::Sd(value)
    }
}

pub struct SdCardManager<SPI: SpiDevice<u8>> {
    bootcount: u8,
    boot_dir_name: String<4>,
    volume_manager: VolumeManager<SdCard<SPI, Delay>, SdTimeSource>,
}

impl<SPI: SpiDevice<u8>> SdCardManager<SPI> {
    pub fn new(spi: SPI) -> Result<Self, SdError<SPI>> {
        let sd_card = SdCard::new(spi, Delay);

        defmt::info!("SD card size is {} bytes", sd_card.num_bytes());

        let volume_manager = VolumeManager::new(sd_card, SdTimeSource);

        let (bootcount, boot_dir_name) = {
            let volume0 = volume_manager.open_volume(VolumeIdx(0))?;
            let root_dir = volume0.open_root_dir()?;

            defmt::info!("Opened root dir");

            // Retrieve boot count
            let bootcount = match root_dir.open_file_in_dir("BOOTCNT", Mode::ReadOnly) {
                Ok(file) => {
                    let mut buf = [0u8; 1];
                    if file.read(&mut buf)? == 1 {
                        buf[0] + 1
                    } else {
                        0
                    }
                }
                Err(embedded_sdmmc::Error::NotFound) => 0,
                Err(e) => return Err(e.into()),
            };

            // Write updated boot count
            let bc_file = root_dir.open_file_in_dir("BOOTCNT", Mode::ReadWriteCreateOrTruncate)?;
            bc_file.write(&[bootcount])?;

            defmt::info!("Bootcount is {}", bootcount);

            let boot_dir_name =
                format!(4; "{:04}", bootcount).expect("should be able to format dir name");

            // Create data dir for this boot
            match root_dir.make_dir_in_dir(boot_dir_name.as_str()) {
                Ok(_) => {}
                Err(embedded_sdmmc::Error::DirAlreadyExists) => {}
                Err(e) => return Err(e.into()),
            }

            (bootcount, boot_dir_name)
        };

        Ok(Self {
            bootcount,
            boot_dir_name,
            volume_manager,
        })
    }

    pub fn volume_manager(&self) -> &VolumeManager<SdCard<SPI, Delay>, SdTimeSource> {
        &self.volume_manager
    }

    pub fn boot_dir_name(&self) -> &str {
        &self.boot_dir_name
    }

    pub fn bootcount(&self) -> u8 {
        self.bootcount
    }
}

pub struct SdFileWriter<'a, SPI: SpiDevice<u8>> {
    file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
}

impl<'a, SPI: SpiDevice<u8>> SdFileWriter<'a, SPI> {
    pub fn new(file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>) -> Self {
        Self { file }
    }
}

impl<'a, SPI: SpiDevice<u8>> ByteSink for SdFileWriter<'a, SPI> {
    type Error = SdError<SPI>;

    async fn sink(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.file.write(buf)?;
        self.file.flush()?;

        Ok(())
    }
}

pub struct SdComponent<'a, 'b, SPI: SpiDevice<u8>> {
    status: Status,
    requests: SdRequestChannelReceiver<'b>,
    buf: [u8; Packet::MAX_ENCODE_BUFFER_SIZE],
    data_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
    logs_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
}

impl<'a, 'b, SPI: SpiDevice<u8>> SdComponent<'a, 'b, SPI> {
    pub fn new(
        requests: SdRequestChannelReceiver<'b>,
        data_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
        logs_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
    ) -> Self {
        Self {
            status: Status::Initialized,
            requests,
            buf: [0; _],
            data_file,
            logs_file,
        }
    }
}

impl<'a, 'b, SPI: SpiDevice<u8>> Component for SdComponent<'a, 'b, SPI> {
    type Error = SdError<SPI>;

    fn id(&self) -> DeviceId {
        DeviceId::System
    }

    fn status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    async fn run_once(
        &mut self,
        _ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        while !self.requests.is_empty() {
            match self.requests.receive().await {
                SdRequest::WritePacket(packet) => {
                    self.data_file.write(
                        packet
                            .encode(&mut self.buf)
                            .map_err(CommunicationError::from)?,
                    )?;
                }
                SdRequest::LogMessage(message) => self.logs_file.write(message.as_bytes())?,
            }
        }

        // Flush both files only after writing all available messages
        self.data_file.flush()?;
        self.logs_file.flush()?;
        defmt::info!("SdComponent flushed");

        Ok(())
    }
}
