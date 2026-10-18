use core::fmt::Debug;

use crate::audio_stream::{AUDIO_FRAME_SIZE, AudioConsumer, AudioReadGrant};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_hal::spi::SpiDevice;
use embedded_sdmmc::{
    BlockDevice, Directory, File, Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager,
};
use heapless::{String, format};
use orbipacket::{DeviceId, Packet, Payload};
use orbisat::{
    Component, Status,
    channels::SdRequestChannelReceiver,
    comms::{ByteSink, CommunicationError},
    sd::SdRequest,
};

const AUDIO_SD_WRITE_CHUNK_SIZE: usize = 512;
const WAV_FRAMES_PER_FILE: u8 = 20;

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
    wav_dir: &'a Directory<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
    wav_num: u16,
    audio_file: Option<File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>>,
    audio_frames: u8,
    audio_consumer: AudioConsumer,
    prefer_audio: bool,
}

impl<'a, 'b, SPI: SpiDevice<u8>> SdComponent<'a, 'b, SPI> {
    pub fn new(
        requests: SdRequestChannelReceiver<'b>,
        data_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
        logs_file: &'a File<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
        wav_dir: &'a Directory<'a, SdCard<SPI, Delay>, SdTimeSource, 4, 4, 1>,
        audio_consumer: AudioConsumer,
    ) -> Self {
        Self {
            status: Status::Initialized,
            requests,
            buf: [0; _],
            data_file,
            logs_file,
            wav_dir,
            wav_num: 0,
            audio_file: None,
            audio_frames: 0,
            audio_consumer,
            prefer_audio: false,
        }
    }

    async fn write_audio_frame(&mut self, frame: AudioReadGrant) -> Result<(), SdError<SPI>> {
        let write_started = Instant::now().as_micros();
        let write_result = async {
            if self.audio_file.is_none() {
                // Unwrapping is safe here because a u16 is exactly four hex digits.
                let fname = heapless::format!(8; "{:04X}.wav", self.wav_num).unwrap();
                let file = self
                    .wav_dir
                    .open_file_in_dir(fname.as_str(), Mode::ReadWriteCreateOrTruncate)?;
                let expected_data_size = u32::from(WAV_FRAMES_PER_FILE) * AUDIO_FRAME_SIZE as u32;
                file.write(&orbisat::sd::wav_header(
                    1,
                    orbisat::sd::SAMPLE_RATE_HZ,
                    16,
                    expected_data_size,
                ))?;
                self.audio_file = Some(file);
            }

            let file = self.audio_file.as_ref().unwrap();
            for chunk in frame.chunks(AUDIO_SD_WRITE_CHUNK_SIZE) {
                file.write(chunk)?;
                Timer::after(Duration::from_micros(100)).await;
            }

            let next_frame_count = self.audio_frames + 1;
            file.flush()?;

            Ok::<u8, SdError<SPI>>(next_frame_count)
        }
        .await;
        let write_duration = Instant::now().as_micros().wrapping_sub(write_started);
        // defmt::warn!(
        //     "WAV file {} frame {} SD write took {} us",
        //     self.wav_num,
        //     self.audio_frames,
        //     write_duration
        // );

        frame.release();
        self.audio_frames = match write_result {
            Ok(frame_count) => frame_count,
            Err(error) => {
                self.audio_file = None;
                self.audio_frames = 0;
                return Err(error);
            }
        };
        if self.audio_frames == WAV_FRAMES_PER_FILE {
            self.audio_file = None;
            self.audio_frames = 0;
            self.wav_num += 1;
        }

        Ok(())
    }

    fn handle_request(&mut self, request: SdRequest) -> Result<(), SdError<SPI>> {
        match request {
            SdRequest::WritePacket(packet) => {
                self.data_file.write(
                    packet
                        .encode(&mut self.buf)
                        .map_err(CommunicationError::from)?,
                )?;
                self.data_file.flush()?;
            }
            SdRequest::LogMessage(message) => {
                self.logs_file.write(message.as_bytes())?;
                self.logs_file.flush()?;
            }
        }

        // defmt::warn!("SdComponent handled request");

        Ok(())
    }

    async fn handle_requests(&mut self, first: SdRequest) -> Result<(), SdError<SPI>> {
        self.handle_request(first)?;

        while !self.requests.is_empty() {
            self.handle_request(self.requests.receive().await)?;
        }

        Ok(())
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

    fn run(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async {
            defmt::info!("Component {} started running", self.id());
            // ctx.log(heapless::format!("Component {} started running\n", self.id()).unwrap())
            // .await;
            self.set_status(Status::Running);

            loop {
                let result = {
                    self.receive_tc(ctx).await?;
                    self.run_once(ctx).await
                };

                match result {
                    Ok(_) => {}
                    Err(e) => {
                        self.set_status(Status::Failed);
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn run_once(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        if self.prefer_audio
            && let Ok(frame) = self.audio_consumer.read()
        {
            self.prefer_audio = false;
            return self.write_audio_frame(frame).await;
        }

        if !self.requests.is_empty() {
            self.prefer_audio = true;
            return self.handle_requests(self.requests.receive().await).await;
        }

        match embassy_futures::select::select(
            self.requests.receive(),
            self.audio_consumer.wait_read(),
        )
        .await
        {
            embassy_futures::select::Either::First(request) => {
                self.prefer_audio = true;
                self.handle_requests(request).await
            }
            embassy_futures::select::Either::Second(frame) => {
                self.prefer_audio = false;
                self.write_audio_frame(frame).await
            }
        }
    }
}
