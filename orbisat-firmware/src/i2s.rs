use core::fmt::Debug;
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Async, Blocking,
    dma::{DmaError, DmaTransferRxCircular},
    gpio::Output,
    i2s::master::I2sRx,
    peripherals::TIMG0,
    spi::master::Spi,
    system,
    time::Duration,
    timer::timg::{self, Wdt},
};
use orbipacket::DeviceId;
use orbisat::{Component, ContextHandle};
use orbisat::{
    Status,
    comms::{ByteSink, CommunicationError},
};
use orbisat_components::sd::SdFileWriter;

#[derive(Clone)]
pub struct AudioChunk {
    pub len: usize,
    pub data: [u8; 32 * 1024],
}

#[derive(thiserror::Error)]
pub enum AudioError {
    #[error("dma error: {0:?}")]
    Dma(DmaError),
    #[error(transparent)]
    Communication(#[from] CommunicationError),
}

impl Debug for AudioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Dma(arg0) => f.debug_tuple("Dma").field(arg0).finish(),
            Self::Communication(arg0) => f.debug_tuple("Communication").field(arg0).finish(),
        }
    }
}

impl From<DmaError> for AudioError {
    fn from(e: DmaError) -> Self {
        AudioError::Dma(e)
    }
}

pub struct AudioRecorderComponent<'a> {
    status: Status,
    transfer: DmaTransferRxCircular<'a, I2sRx<'a, Blocking>>,
    writer: SdFileWriter<'a, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>,
    wdt: &'a mut Wdt<TIMG0<'static>>,
}

impl<'a> AudioRecorderComponent<'a> {
    pub fn new(
        transfer: DmaTransferRxCircular<'a, I2sRx<'a, Blocking>>,
        writer: SdFileWriter<'a, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>,
        wdt: &'a mut Wdt<TIMG0<'static>>,
    ) -> Self {
        Self {
            status: Status::Initialized,
            transfer,
            writer,
            wdt,
        }
    }
}

impl<'a> Component for AudioRecorderComponent<'a> {
    type Error = AudioError;

    fn id(&self) -> DeviceId {
        DeviceId::Mission2
    }

    fn status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    async fn run_once(&mut self, _ctx: &mut ContextHandle<'_>) -> Result<(), Self::Error> {
        let mut data = [0u8; 4 * 1024];
        let mut available = self.transfer.available()?;
        while available < 2 * 1024 {
            embassy_futures::yield_now().await;
            available = self.transfer.available()?;
        }

        let filled = self.transfer.pop(&mut data)?;

        defmt::info!("Writing {} bytes", filled);
        // TODO: errors
        self.writer.sink(&data[..filled]).await.unwrap();
        embassy_futures::yield_now().await;

        Ok(())
    }
}
