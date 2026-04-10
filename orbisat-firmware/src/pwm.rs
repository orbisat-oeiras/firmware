use embedded_hal::pwm::{ErrorType, SetDutyCycle};
use esp_hal::{
    gpio::DriveMode,
    ledc::{
        LSGlobalClkSource, Ledc, LowSpeed,
        channel::{
            self, Channel, ChannelIFace, Number as ChannelNumber, config::Config as ChannelConfig,
        },
        timer::{
            LSClockSource, Number as TimerNumber, Timer, TimerHW, TimerIFace,
            config::{Config as LedcTimerConfig, Duty},
        },
    },
    peripherals::{GPIO33, LEDC},
    time::Rate,
};
use orbisat_components::secondary::SetFrequency;
use static_cell::StaticCell;

pub struct PwmController {
    _ledc: &'static mut Ledc<'static>,
    lstimer0: &'static Timer<'static, LowSpeed>,
    channel0: &'static mut Channel<'static, LowSpeed>,
}

impl PwmController {
    #[cfg(feature = "esp32")]
    pub fn new(ledc_peripheral: LEDC<'static>, gpio_peripheral: GPIO33<'static>) -> Self {
        static LEDC: StaticCell<Ledc<'static>> = StaticCell::new();
        static LSTIMER0: StaticCell<Timer<'static, LowSpeed>> = StaticCell::new();
        static CHANNEL0: StaticCell<Channel<'static, LowSpeed>> = StaticCell::new();

        let ledc = LEDC.init(Ledc::new(ledc_peripheral));
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        let lstimer0 = LSTIMER0.init(ledc.timer::<LowSpeed>(TimerNumber::Timer0));
        lstimer0
            .configure(LedcTimerConfig {
                duty: Duty::Duty1Bit,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_hz(1000),
            })
            .expect("should be able to configure ledc timer");

        let lstimer0 = &(*lstimer0);

        let channel0 = CHANNEL0.init(ledc.channel(ChannelNumber::Channel0, gpio_peripheral));
        channel0
            .configure(ChannelConfig {
                timer: lstimer0,
                drive_mode: DriveMode::PushPull,
                duty_pct: 50,
            })
            .expect("should be able to configure ledc channel");

        Self {
            _ledc: ledc,
            lstimer0,
            channel0,
        }
    }

    #[cfg(feature = "esp32s3")]
    pub fn new(ledc_peripheral: LEDC<'static>, gpio_peripheral: GPIO34) -> Self {
        static LEDC: StaticCell<Ledc<'static>> = StaticCell::new();
        static LSTIMER0: StaticCell<Timer<'static, LowSpeed>> = StaticCell::new();
        static CHANNEL0: StaticCell<Channel<'static, LowSpeed>> = StaticCell::new();

        let ledc = LEDC.init(Ledc::new(ledc_peripheral));
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        let lstimer0 = LSTIMER0.init(ledc.timer::<LowSpeed>(TimerNumber::Timer0));
        lstimer0
            .configure(LedcTimerConfig {
                duty: Duty::Duty1Bit,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_hz(1000),
            })
            .expect("should be able to configure ledc timer");

        let lstimer0 = &(*lstimer0);

        let channel0 = CHANNEL0.init(ledc.channel(ChannelNumber::Channel0, gpio_peripheral));
        channel0
            .configure(ChannelConfig {
                timer: lstimer0,
                drive_mode: DriveMode::PushPull,
                duty_pct: 50,
            })
            .expect("should be able to configure ledc channel");

        Self {
            ledc,
            lstimer0,
            channel0,
        }
    }
}

impl SetFrequency for PwmController {
    fn set_divider(&mut self, divider: u32) {
        self.lstimer0.configure_hw(divider);
    }
}

impl ErrorType for PwmController {
    type Error = channel::Error;
}

impl SetDutyCycle for PwmController {
    fn max_duty_cycle(&self) -> u16 {
        self.channel0.max_duty_cycle()
    }

    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
        self.channel0.set_duty_cycle(duty)
    }
}
