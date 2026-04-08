use embedded_hal::pwm::{self, SetDutyCycle};
use orbipacket::DeviceId;
use orbisat::{Component, comms::CommunicationError};

#[derive(thiserror::Error)]
pub enum SpeakerError<PWM, E>
where
    E: pwm::Error,
    PWM: pwm::ErrorType<Error = E>,
{
    #[error(transparent)]
    Communication(#[from] CommunicationError),
    #[error("pwm error: {0:?}")]
    Pwm(PWM::Error),
}

impl<PWM, E> core::fmt::Debug for SpeakerError<PWM, E>
where
    E: pwm::Error,
    PWM: pwm::ErrorType<Error = E>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            SpeakerError::Communication(f0) => f.debug_tuple("Communication").field(&f0).finish(),
            SpeakerError::Pwm(f0) => f.debug_tuple("Pwm").field(&f0).finish(),
        }
    }
}

impl<PWM, E> From<E> for SpeakerError<PWM, E>
where
    E: pwm::Error,
    PWM: pwm::ErrorType<Error = E>,
{
    fn from(value: PWM::Error) -> Self {
        Self::Pwm(value)
    }
}

#[derive(Debug)]
pub struct SpeakerComponent<PWM: SetDutyCycle> {
    pwm: PWM,
}

impl<PWM: SetDutyCycle> SpeakerComponent<PWM> {
    pub fn new(pwm: PWM) -> Self {
        Self { pwm }
    }
}

impl<PWM: SetDutyCycle> Component for SpeakerComponent<PWM> {
    type Error = SpeakerError<PWM, PWM::Error>;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Mission1
    }

    async fn run(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.pwm.set_duty_cycle(self.pwm.max_duty_cycle() / 2)?;
        loop {
            self.receive_tc(ctx).await?;
            self.run_once(ctx).await?;
        }
    }

    async fn run_once(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        ctx.next_tick().await;
        Ok(())
    }
}
