use esp_hal::{
    Async,
    i2s::master::{I2sRx, asynch::I2sReadDmaTransferAsync},
};
use orbisat::{
    Component,
    comms::CommunicationError,
    sd::{SdRequest, WavFile},
};

#[derive(thiserror::Error, Debug)]
pub enum MicError {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
}

pub struct MicComponent<'a> {
    status: orbisat::Status,
    i2s_rx: Option<I2sRx<'a, Async>>,
    dma_rx_buffer: Option<&'a mut [u8]>,
    transaction: Option<I2sReadDmaTransferAsync<'a, &'a mut [u8]>>,
    // TODO: don't hardcode this
    buf: [u8; 8192],
    rx_buf: &'a mut [u8; 1024],
    idx: usize,
}

impl<'a> MicComponent<'a> {
    pub fn new(
        i2s_rx: I2sRx<'a, Async>,
        dma_rx_buffer: &'a mut [u8],
        rx_buf: &'a mut [u8; 1024],
    ) -> Self {
        Self {
            status: orbisat::Status::Initialized,
            i2s_rx: Some(i2s_rx),
            dma_rx_buffer: Some(dma_rx_buffer),
            transaction: None,
            buf: [0; _],
            rx_buf,
            idx: 0,
        }
    }
}

impl<'a> Component for MicComponent<'a> {
    type Error = MicError;

    fn id(&self) -> orbipacket::DeviceId {
        orbipacket::DeviceId::Mission2
    }

    fn status(&self) -> orbisat::Status {
        self.status
    }

    fn set_status(&mut self, status: orbisat::Status) {
        self.status = status;
    }

    async fn run_once(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        if let Some(transaction) = &mut self.transaction {
            // TODO: error handling
            let count = transaction.pop(self.rx_buf).await.unwrap();

            defmt::warn!("Popped {} bytes, idx is at {}", count, self.idx);

            if self.idx + count >= self.buf.len() {
                let capacity = self.buf.len() - self.idx;
                defmt::warn!("Copying {} bytes", capacity);
                self.buf[self.idx..].copy_from_slice(&self.rx_buf[..capacity]);

                // TODO: don't hardcode the values
                ctx.sd_request(SdRequest::WriteWav(WavFile::new(2, 44100, 16, self.buf)))
                    .await;

                self.buf[..].copy_from_slice(&self.rx_buf[capacity..]);
                self.idx = capacity;
                defmt::warn!("Wav request sent, idx is at {}", self.idx);
            } else {
                self.buf[self.idx..].copy_from_slice(&self.rx_buf[..count]);
                self.idx += count;
                defmt::warn!("Copied all bytes, idx is at {}", self.idx);
            }
        }

        Ok(())
    }

    async fn run(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.set_status(orbisat::Status::Running);

        self.transaction = if let Some(i2s_rx) = self.i2s_rx.take() {
            i2s_rx
                .read_dma_circular_async(self.dma_rx_buffer.take().unwrap())
                .ok()
        } else {
            None
        };

        loop {
            let result = {
                self.receive_tc(ctx).await?;
                self.run_once(ctx).await
            };

            match result {
                Ok(_) => {}
                Err(e) => {
                    self.set_status(orbisat::Status::Failed);
                    return Err(e);
                }
            }
        }
    }
}
