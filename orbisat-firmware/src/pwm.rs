use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
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
use static_cell::StaticCell;

#[cfg(feature = "esp32")]
type PwmPin = GPIO33<'static>;
#[cfg(feature = "esp32s3")]
type PwmPin = GPIO34<'static>;

pub struct PwmController {
    ledc: &'static mut Ledc<'static>,
    lstimer0: Mutex<CriticalSectionRawMutex, &'static mut LedcTimer<'static, LowSpeed>>,
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
                duty,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_hz(1000),
            })
            .expect("should be able to configure ledc timer");

        let lstimer0 = Mutex::new(lstimer0);

        Self {
            ledc,
            lstimer0,
            pin,
            duty,
        }
    }
}

impl SetFrequency for PwmController {
    async fn set_frequency(&mut self, frequency: u32) {
        unsafe {
            self.lstimer0.lock_mut(|lstimer0| {
                lstimer0
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
                        timer: *lstimer0,
                        drive_mode: DriveMode::PushPull,
                        duty_pct: 50,
                    })
                    .expect("should be able to configure ledc channel");
            });
        }
    }
}
