#[cfg(feature = "esp32")]
use esp_hal::peripherals::GPIO33;
#[cfg(feature = "esp32s3")]
use esp_hal::peripherals::GPIO34;
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{ChannelIFace, Number as ChannelNumber, config::Config as ChannelConfig},
        timer::{
            LSClockSource, Number as TimerNumber, Timer as LedcTimer, TimerIFace,
            config::{Config as TimerConfig, Duty},
        },
    },
    peripherals::LEDC,
    time::Rate,
};
use orbisat_components::secondary::SetFrequency;

#[cfg(feature = "esp32")]
type PwmPin = GPIO33<'static>;
#[cfg(feature = "esp32s3")]
type PwmPin = GPIO34<'static>;

pub struct PwmController<'a> {
    ledc: Ledc<'a>,
    lstimer0: LedcTimer<'a, LowSpeed>,
    pin: PwmPin,
    duty: Duty,
}

impl<'a> PwmController<'a> {
    pub fn new(ledc_peripheral: LEDC<'a>, pin: PwmPin, duty: Duty) -> Self {
        let mut ledc = Ledc::new(ledc_peripheral);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        let lstimer0 = ledc.timer::<LowSpeed>(TimerNumber::Timer0);

        Self {
            ledc,
            lstimer0,
            pin,
            duty,
        }
    }
}

impl<'a> SetFrequency for PwmController<'a> {
    async fn set_frequency(&mut self, frequency: u32) {
        self.lstimer0
            .configure(TimerConfig {
                clock_source: LSClockSource::APBClk,
                duty: self.duty,
                frequency: Rate::from_hz(frequency),
            })
            .expect("should be able to configure ledc timer");

        let mut channel0 = self
            .ledc
            .channel(ChannelNumber::Channel0, self.pin.reborrow());
        channel0
            .configure(ChannelConfig {
                timer: &self.lstimer0,
                drive_mode: DriveMode::PushPull,
                duty_pct: 50,
            })
            .expect("should be able to configure ledc channel");
    }

    async fn stop(&mut self) {
        let mut channel0 = self
            .ledc
            .channel(ChannelNumber::Channel0, self.pin.reborrow());
        channel0
            .configure(ChannelConfig {
                timer: &self.lstimer0,
                drive_mode: DriveMode::PushPull,
                duty_pct: 0,
            })
            .expect("should be able to configure ledc channel");
    }
}
