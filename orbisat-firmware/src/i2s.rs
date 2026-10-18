use embassy_time::{Duration, Timer};
use esp_hal::{
    Async,
    dma::DmaRxStreamBuf,
    i2s::master::{I2sRx, I2sRxDmaTransfer},
};
use orbisat::{Component, comms::CommunicationError};
use orbisat_components::audio_stream::{AUDIO_FRAME_SIZE, AudioProducer, AudioWriteGrant};

#[derive(thiserror::Error, Debug)]
pub enum MicError {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
}

pub struct MicComponent<'a> {
    status: orbisat::Status,
    i2s_rx: Option<I2sRx<'a, Async>>,
    dma_rx_buffer: Option<DmaRxStreamBuf>,
    transaction: Option<I2sRxDmaTransfer<'a, Async, DmaRxStreamBuf>>,
    rx_buf: &'a mut [u8; 1024],
    audio_producer: AudioProducer,
    audio_frame: Option<AudioWriteGrant>,
    restart_pending: bool,
    idx: usize,
}

impl<'a> MicComponent<'a> {
    pub fn new(
        i2s_rx: I2sRx<'a, Async>,
        dma_rx_buffer: DmaRxStreamBuf,
        rx_buf: &'a mut [u8; 1024],
        audio_producer: AudioProducer,
    ) -> Self {
        Self {
            status: orbisat::Status::Initialized,
            i2s_rx: Some(i2s_rx),
            dma_rx_buffer: Some(dma_rx_buffer),
            transaction: None,
            rx_buf,
            audio_producer,
            audio_frame: None,
            restart_pending: false,
            idx: 0,
        }
    }

    fn start_dma(&mut self) {
        let Some(i2s_rx) = self.i2s_rx.take() else {
            return;
        };
        let Some(buffer) = self.dma_rx_buffer.take() else {
            self.i2s_rx = Some(i2s_rx);
            return;
        };

        match i2s_rx.read(buffer) {
            Ok(transaction) => self.transaction = Some(transaction),
            Err((error, i2s_rx, buffer)) => {
                defmt::warn!("I2S DMA start failed: {}", error);
                self.i2s_rx = Some(i2s_rx);
                self.dma_rx_buffer = Some(buffer);
            }
        }
    }

    fn restart_dma(&mut self, transaction: I2sRxDmaTransfer<'a, Async, DmaRxStreamBuf>) {
        let (i2s_rx, buffer) = transaction.stop();
        self.i2s_rx = Some(i2s_rx);
        self.dma_rx_buffer = Some(buffer);
        self.start_dma();
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
        _ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        if self.transaction.is_none() {
            self.start_dma();
        }

        if let Some(transaction) = &mut self.transaction {
            while transaction.available_bytes() == 0 && !transaction.is_done() {
                Timer::after(Duration::from_millis(1)).await;
            }

            if transaction.is_done() {
                self.restart_pending = true;
            }

            let count = transaction.pop(self.rx_buf);
            let mut copied = 0;
            while copied < count {
                if self.audio_frame.is_none() {
                    // defmt::warn!("Mic waiting for free audio queue frame");
                    self.audio_frame = Some(
                        self.audio_producer
                            .wait_grant(AUDIO_FRAME_SIZE as u16)
                            .await,
                    );
                }

                let remaining = AUDIO_FRAME_SIZE - self.idx;
                let chunk = remaining.min(count - copied);
                let frame = self.audio_frame.as_mut().unwrap();
                frame[self.idx..self.idx + chunk]
                    .copy_from_slice(&self.rx_buf[copied..copied + chunk]);
                self.idx += chunk;
                copied += chunk;

                if self.idx == AUDIO_FRAME_SIZE {
                    self.audio_frame
                        .take()
                        .unwrap()
                        .commit(AUDIO_FRAME_SIZE as u16);
                    self.idx = 0;
                }
            }
        }

        let restart = self.restart_pending
            && self
                .transaction
                .as_mut()
                .is_none_or(|transaction| transaction.available_bytes() == 0);
        if restart {
            // defmt::warn!("Restarting drained I2S RX transfer");
            if let Some(transaction) = self.transaction.take() {
                self.restart_dma(transaction);
            }
            self.restart_pending = false;
        }

        Timer::after(Duration::from_micros(100)).await;
        Ok(())
    }

    async fn run(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.set_status(orbisat::Status::Running);

        self.start_dma();
        Timer::after(Duration::from_micros(100)).await;
        loop {
            let result = {
                self.receive_tc(ctx).await?;
                self.run_once(ctx).await
            };

            match result {
                Ok(_) => {}
                Err(error) => {
                    self.set_status(orbisat::Status::Failed);
                    return Err(error);
                }
            }
        }
    }
}
