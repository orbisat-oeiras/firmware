use embassy_time::{Duration, Timer};
use orbipacket::DeviceId;
use orbisat::{Component, comms::CommunicationError};

#[derive(thiserror::Error)]
pub enum SpeakerError {
    #[error(transparent)]
    Communication(#[from] CommunicationError),
}

impl core::fmt::Debug for SpeakerError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            SpeakerError::Communication(f0) => f.debug_tuple("Communication").field(&f0).finish(),
        }
    }
}

pub trait SetFrequency {
    fn set_frequency(&mut self, frequency: u32) -> impl Future<Output = ()>;
}

type SweepData<'a> = &'a [(u32, u64)];

#[derive(Debug)]
pub struct SpeakerComponent<'a, PWM: SetFrequency> {
    pwm: PWM,
    data: SweepData<'a>,
}

impl<'a, PWM: SetFrequency> SpeakerComponent<'a, PWM> {
    pub fn new(pwm: PWM, data: SweepData<'a>) -> Self {
        Self { pwm, data }
    }
}

impl<'a, PWM: SetFrequency> Component for SpeakerComponent<'a, PWM> {
    type Error = SpeakerError;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Mission1
    }

    async fn run_once(&mut self, _ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        for (frequency, duration) in self.data {
            self.pwm.set_frequency(*frequency).await;
            Timer::after(Duration::from_micros(*duration)).await;
        }

        Ok(())
    }
}
