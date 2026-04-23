use core::fmt::Debug;
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Async, Blocking,
    dma::{DmaError, DmaTransferRxCircular},
    gpio::Output,
    i2s::master::I2sRx,
    spi::master::Spi,
};
use orbipacket::DeviceId;
use orbisat::comms::CommunicationError;
use orbisat::{Component, ContextHandle};
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
    transfer: DmaTransferRxCircular<'a, I2sRx<'a, Blocking>>,
    writer: SdFileWriter<'a, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>,
}

impl<'a> AudioRecorderComponent<'a> {
    pub fn new(
        transfer: DmaTransferRxCircular<'a, I2sRx<'a, Blocking>>,
        writer: SdFileWriter<'a, ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>>,
    ) -> Self {
        Self { transfer, writer }
    }
}

impl<'a> Component for AudioRecorderComponent<'a> {
    type Error = AudioError;

    fn id(&self) -> DeviceId {
        DeviceId::Mission2
    }

    async fn run_once(&mut self, ctx: &mut ContextHandle<'_>) -> Result<(), Self::Error> {
        let mut data = [0u8; 32 * 1024];
        let mut filled = 0;

        let mut write_slice = &mut data[..];
        while write_slice.len() > 4 * 1024 {
            let available = self.transfer.available()?;

            if available > 0 {
                let to_copy = available.min(write_slice.len());
                self.transfer.pop(write_slice)?;
                write_slice = &mut write_slice[to_copy..];
                filled += to_copy;
            }
            embassy_futures::yield_now().await;
        }

        defmt::info!("Writing {} bytes", filled);
        // TODO: errors
        self.writer.write(&data[..filled], ctx).await.unwrap();
        Ok(())
    }
}
