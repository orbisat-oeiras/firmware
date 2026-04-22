use core::marker::PhantomData;

use embassy_time::{Delay, Instant};
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{BlockDevice, Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};

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
}

impl<SPI: SpiDevice<u8>> From<embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>>
    for SdError<SPI>
{
    fn from(value: embedded_sdmmc::Error<<SdCard<SPI, Delay> as BlockDevice>::Error>) -> Self {
        Self::Sd(value)
    }
}

pub struct SdCardManager<SPI: SpiDevice<u8>> {
    // volume_manager: VolumeManager<SdCard<SPI, Delay>, SdTimeSource>,
    _spi: core::marker::PhantomData<SPI>,
}

impl<SPI: SpiDevice<u8>> SdCardManager<SPI> {
    pub fn new(spi: SPI) -> Result<Self, SdError<SPI>> {
        let sd_card = SdCard::new(spi, Delay);

        defmt::info!("SD card size is {} bytes", sd_card.num_bytes());

        let volume_manager = VolumeManager::new(sd_card, SdTimeSource);
        let volume0 = volume_manager.open_volume(VolumeIdx(0))?;

        let root_dir = volume0.open_root_dir()?;
        let file = root_dir.open_file_in_dir("TEST.TXT", Mode::ReadWriteCreateOrTruncate)?;

        file.write(b"Hello, world!")?;
        file.flush()?;
        defmt::info!("Wrote to file");

        Ok(Self { _spi: PhantomData })
    }
}
