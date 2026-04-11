use embassy_time::Duration;
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
    fn set_divider(&mut self, divider: u32);

    fn play_frequency_for_duration(
        &mut self,
        frequency: f32,
        duration: Duration,
    ) -> impl Future<Output = ()>;

    fn set_frequency_integer(&mut self, frequency: u32);

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

type SweepData<'a> = &'a [(f32, u64)];

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
            self.pwm
                .play_frequency_for_duration(*frequency, Duration::from_micros(*duration))
                .await;
        }

        Ok(())
    }
}
