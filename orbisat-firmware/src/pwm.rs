use embassy_time::Timer;
#[cfg(feature = "esp32")]
use esp_hal::peripherals::GPIO33;
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{ChannelIFace, Number as ChannelNumber, config::Config as ChannelConfig},
        timer::{
            LSClockSource, Number as TimerNumber, Timer as LedcTimer, TimerHW, TimerIFace,
            config::{Config as TimerConfig, Duty},
        },
    },
    peripherals::LEDC,
    time::Rate,
};
use orbisat_components::secondary::SetFrequency;
use static_cell::StaticCell;

#[cfg(feature = "esp32")]
type PwmPin = GPIO33<'static>;
#[cfg(feature = "esp32s3")]
type PwmPin = GPIO34<'static>;

pub struct PwmController {
    ledc: &'static mut Ledc<'static>,
    lstimer0: &'static LedcTimer<'static, LowSpeed>,
    pin: PwmPin,
    duty: Duty,
}

impl PwmController {
    pub fn new(ledc_peripheral: LEDC<'static>, pin: PwmPin, duty: Duty) -> Self {
        static LEDC: StaticCell<Ledc<'static>> = StaticCell::new();
        static LSTIMER0: StaticCell<LedcTimer<'static, LowSpeed>> = StaticCell::new();

        let ledc = LEDC.init(Ledc::new(ledc_peripheral));
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        let lstimer0 = LSTIMER0.init(ledc.timer::<LowSpeed>(TimerNumber::Timer0));
        lstimer0
            .configure(TimerConfig {
                duty: Duty::Duty1Bit,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_hz(1000),
            })
            .expect("should be able to configure ledc timer");

        let lstimer0 = &(*lstimer0);

        Self {
            ledc,
            lstimer0,
            pin,
            duty,
        }
    }
}

impl SetFrequency for PwmController {
    fn set_divider(&mut self, divider: u32) {
        self.lstimer0.configure_hw(divider);
    }

    fn set_frequency_integer(&mut self, frequency: u32) {
        // from <Timer as TimerIFace>::configure()
        let src_freq: u32 = self
            .lstimer0
            .freq()
            .expect("lstimer0 source frequency should be set")
            .as_hz();
        let precision = 1 << self.duty as u32;
        let divider = ((src_freq as u64) << 8) / frequency as u64 / precision as u64;
        self.set_divider(divider as u32);
    }

    async fn play_frequency_for_duration(
        &mut self,
        frequency: f32,
        duration: embassy_time::Duration,
    ) {
        self.set_frequency_integer(frequency as u32);

        let mut channel0 = self
            .ledc
            .channel(ChannelNumber::Channel0, self.pin.reborrow());
        channel0
            .configure(ChannelConfig {
                timer: self.lstimer0,
                drive_mode: DriveMode::PushPull,
                duty_pct: 50,
            })
            .expect("should be able to configure ledc channel");

        Timer::after(duration).await;
    }
}
