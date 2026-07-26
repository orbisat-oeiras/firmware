use embassy_time::{Duration, Timer};
use orbipacket::{DeviceId, TimestampError};
use orbisat::{Component, Status, comms::CommunicationError};

#[derive(thiserror::Error, Debug)]
pub enum SpeakerError {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error(transparent)]
    Format(#[from] core::fmt::Error),
}

impl From<TimestampError> for SpeakerError {
    fn from(value: TimestampError) -> Self {
        Self::Communication(CommunicationError::Timestamp(value))
    }
}

pub trait SetFrequency {
    fn set_frequency(&mut self, frequency: u32) -> impl Future<Output = ()>;

    fn stop(&mut self) -> impl Future<Output = ()>;
}

type SweepData<'a> = &'a [(u32, u64)];

pub struct SpeakerComponent<'a, PWM: SetFrequency> {
    status: Status,
    pwm: PWM,
    data: SweepData<'a>,
    idx: usize,
    forward_sweep: bool,
}

impl<'a, PWM: SetFrequency> SpeakerComponent<'a, PWM> {
    pub fn new(pwm: PWM, data: SweepData<'a>) -> Self {
        Self {
            status: Status::Initialized,
            pwm,
            data,
            idx: 0,
            forward_sweep: true,
        }
    }
}

impl<'a, PWM: SetFrequency> Component for SpeakerComponent<'a, PWM> {
    type Error = SpeakerError;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Mission1
    }

    fn status(&self) -> Status {
        self.status
    }

    fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    async fn run(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.set_status(Status::Paused);

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

    async fn run_once(
        &mut self,
        ctx: &mut orbisat::context::ContextHandle<'_>,
    ) -> Result<(), Self::Error> {
        if self.status == Status::Running {
            self.pwm.set_frequency(self.data[self.idx].0).await;
            Timer::after(Duration::from_micros(self.data[self.idx].1)).await;

            // TODO: The way this is implemented means the timestamp of the
            // first forward sweep doesn't get logged. That's really not that
            // big of an issue, but it'd be nice to fix it if possible.
            if self.forward_sweep {
                if self.idx >= self.data.len() - 1 {
                    ctx.log(
                        heapless::format!(128; "REVERSE SWEEP TIMESTAMP {}\n", &ctx.timestamp()?.get())?
                    ).await;
                    defmt::info!("Starting reverse sweep");

                    self.forward_sweep = false;
                } else {
                    self.idx += 1;
                }
            } else {
                if self.idx == 0 {
                    ctx.log(
                        heapless::format!(128; "FORWARD SWEEP TIMESTAMP {}\n", &ctx.timestamp()?.get())?
                    ).await;
                    defmt::info!("Starting forward sweep");

                    self.forward_sweep = true;
                } else {
                    self.idx -= 1;
                }
            }
        } else {
            embassy_futures::yield_now().await;
        }

        Ok(())
    }

    async fn handle_tc(
        &mut self,
        _ctx: &mut orbisat::context::ContextHandle<'_>,
        _tc: orbipacket::TcPacket,
    ) -> Result<(), Self::Error> {
        match self.status {
            Status::Running => {
                self.set_status(Status::Paused);
                self.pwm.stop().await;
            }
            Status::Paused => {
                self.set_status(Status::Running);
            }
            _ => {}
        }

        Ok(())
    }
}
