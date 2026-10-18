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
    rx_residual: usize,
    x_prev: f32,
    y_prev: f32,
    warmup_samples: usize,
    current_peak: i16,
    frames_committed: u32,
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
            rx_residual: 0,
            x_prev: 0.0,
            y_prev: 0.0,
            warmup_samples: 3200,
            current_peak: 0,
            frames_committed: 0,
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

            let popped = transaction.pop(&mut self.rx_buf[self.rx_residual..]);
            let total_bytes = self.rx_residual + popped;
            let samples_to_process = total_bytes / 8;
            let bytes_processed = samples_to_process * 8;
            let new_residual = total_bytes - bytes_processed;

            for i in 0..samples_to_process {
                let offset = i * 8;
                let l_raw = i32::from_le_bytes([
                    self.rx_buf[offset],
                    self.rx_buf[offset + 1],
                    self.rx_buf[offset + 2],
                    self.rx_buf[offset + 3],
                ]);
                let r_raw = i32::from_le_bytes([
                    self.rx_buf[offset + 4],
                    self.rx_buf[offset + 5],
                    self.rx_buf[offset + 6],
                    self.rx_buf[offset + 7],
                ]);

                // INMP441 puts 24-bit MSB-aligned data in the upper 24 bits of a 32-bit slot
                let l_val = l_raw >> 8;
                let r_val = r_raw >> 8;

                // Dynamically select the active channel (supports L/R tied to GND, 3.3V, or floating)
                let s = if l_val.abs() >= r_val.abs() {
                    l_val
                } else {
                    r_val
                };

                // DC-blocking filter (single-pole high-pass): y[n] = x[n] - x[n-1] + 0.995 * y[n-1]
                let x = s as f32;
                let y = x - self.x_prev + 0.995f32 * self.y_prev;
                self.x_prev = x;
                self.y_prev = y;

                // Discard initial samples to eliminate power-on thump
                if self.warmup_samples > 0 {
                    self.warmup_samples -= 1;
                    continue;
                }

                // Convert 24-bit to 16-bit PCM (/ 256) with +18 dB gain (* 8): scale = 8 / 256 = 1 / 32
                let v = y * (8.0f32 / 256.0f32);
                let sample16 = if v > 32767.0f32 {
                    32767i16
                } else if v < -32768.0f32 {
                    -32768i16
                } else {
                    v as i16
                };

                let mag = sample16.unsigned_abs() as i16;
                if mag > self.current_peak {
                    self.current_peak = mag;
                }

                if self.audio_frame.is_none() {
                    self.audio_frame = Some(
                        self.audio_producer
                            .wait_grant(AUDIO_FRAME_SIZE as u16)
                            .await,
                    );
                }

                let pcm_bytes = sample16.to_le_bytes();
                let frame = self.audio_frame.as_mut().unwrap();
                frame[self.idx] = pcm_bytes[0];
                frame[self.idx + 1] = pcm_bytes[1];
                self.idx += 2;

                if self.idx == AUDIO_FRAME_SIZE {
                    self.audio_frame
                        .take()
                        .unwrap()
                        .commit(AUDIO_FRAME_SIZE as u16);
                    self.idx = 0;
                    self.frames_committed = self.frames_committed.wrapping_add(1);
                    if self.frames_committed % 4 == 0 {
                        defmt::info!("Mic Audio: ACTIVE (peak={}/32767)", self.current_peak);
                        self.current_peak = 0;
                    }
                }
            }

            if new_residual > 0 {
                self.rx_buf.copy_within(bytes_processed..total_bytes, 0);
            }
            self.rx_residual = new_residual;
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
