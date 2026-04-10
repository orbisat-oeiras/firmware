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

pub trait SetFrequency {
    fn set_divider(&mut self, divider: u32);

    fn set_frequency(&mut self, frequency: f32) {
        let (a, b) = self.calculate_ledc_timer_divider(80_000_000, frequency, 1);
        let divider = self.extract_fixed_point_divider(a, b);

        self.set_divider(divider);
    }

    // Based on https://github.com/CastilloDelSol/ESP32_EnhancedPWM/blob/f07942bbde7d2e789ae48caf7e2f6881acfe2229/src/ESP32_EnhancedPWM.h#L238
    fn calculate_ledc_timer_divider(
        &self,
        clock_frequency: u32,
        target_frequency: f32,
        duty_res: u8,
    ) -> (u32, u32) {
        let divider_target = clock_frequency as f32 / (target_frequency * (1 << duty_res) as f32);
        let divider_target = if divider_target > 1. {
            1.
        } else {
            divider_target
        };

        let mut a = divider_target as u32;
        let frac = divider_target - a as f32;
        let mut b = (frac * 256. + 0.5) as u32;

        if b >= 256 {
            a += 1;
            b = 0;
        }

        if a > 1023 {
            a = 1023;
            b = 255;
        }

        (a, b)
    }

    fn extract_fixed_point_divider(&self, a: u32, b: u32) -> u32 {
        (a << 8) | (b & 0xff)
    }
}

#[derive(Debug)]
pub struct SpeakerComponent<PWM: SetDutyCycle + SetFrequency> {
    pwm: PWM,
}

impl<PWM: SetDutyCycle + SetFrequency> SpeakerComponent<PWM> {
    pub fn new(pwm: PWM) -> Self {
        Self { pwm }
    }
}

impl<PWM: SetDutyCycle + SetFrequency> Component for SpeakerComponent<PWM> {
    type Error = SpeakerError<PWM, PWM::Error>;

    fn id(&self) -> orbipacket::DeviceId {
        DeviceId::Mission1
    }

    async fn run(&mut self, ctx: &mut orbisat::ContextHandle<'_>) -> Result<(), Self::Error> {
        defmt::info!("Component {} started running", self.id());
        self.pwm.set_frequency(1000.5);
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
