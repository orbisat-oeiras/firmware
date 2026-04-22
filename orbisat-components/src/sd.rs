use embassy_time::{Delay, Instant};
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{BlockDevice, Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};
use heapless::{String, format};
use orbisat::comms::ByteSink;

#[derive(Debug)]
struct SdTimeSource;

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

#[derive(thiserror::Error, Debug)]
pub enum SdError<SPI: SpiDevice<u8>> {
    #[error("sd error: {0:?}")]
    Sd(embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>),
    #[error("bootcount file cannot be read")]
    BootcountUnreadable,
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
                        return Err(SdError::BootcountUnreadable);
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
}

pub struct SdByteSink<'a, SPI: SpiDevice<u8>>(&'a SdCardManager<SPI>);

impl<'a, SPI: SpiDevice<u8>> SdByteSink<'a, SPI> {
    pub fn new(sd_card_manager: &'a SdCardManager<SPI>) -> Self {
        Self(sd_card_manager)
    }
}

impl<'a, SPI: SpiDevice<u8>> ByteSink for SdByteSink<'a, SPI> {
    type Error = SdError<SPI>;

    async fn sink(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0
            .volume_manager
            .open_volume(VolumeIdx(0))?
            .open_root_dir()?
            .open_dir(self.0.boot_dir_name.as_str())?
            .open_file_in_dir("DATA", Mode::ReadWriteCreateOrAppend)?
            .write(buf)?;

        Ok(())
    }
}
