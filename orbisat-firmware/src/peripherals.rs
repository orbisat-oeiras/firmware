#[cfg(feature = "esp32")]
use esp_hal::peripherals::{GPIO3, GPIO5, GPIO16, GPIO17, GPIO18, GPIO19, GPIO21, GPIO22, GPIO33};
use esp_hal::{
    Async, Blocking,
    i2c::master::{Config as I2cConfig, I2c},
    ledc::timer::config::Duty,
    peripherals::Peripherals,
    uart::{Config as UartConfig, Uart},
};
#[cfg(feature = "esp32s3")]
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    peripherals::{
        GPIO1, GPIO2, GPIO3, GPIO5, GPIO8, GPIO9, GPIO10, GPIO11, GPIO12, GPIO13, GPIO21, GPIO34,
        GPIO42,
    },
    spi::{
        Mode as SpiMode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};

use crate::pwm::PwmController;

pub struct PeripheralManager {
    uart0: Option<Uart<'static, Async>>,
    uart1: Option<Uart<'static, Async>>,
    i2c0: Option<I2c<'static, Async>>,
    i2c1: Option<I2c<'static, Blocking>>,
    pwm: Option<PwmController<'static>>,
    #[cfg(feature = "esp32s3")]
    spi: Option<Spi<'static, Async>>,
    #[cfg(feature = "esp32s3")]
    spi_cs: Option<Output<'static>>,
}

impl PeripheralManager {
    pub fn new(p: Peripherals) -> Self {
        let pins = PinSet::new(&p);

        let uart0 = Uart::new(p.UART2, UartConfig::default().with_baudrate(19200))
            .expect("should be able to construct Uart0")
            .with_rx(pins.uart0_rx)
            .with_tx(pins.uart0_tx)
            .into_async();

        let uart1 = Uart::new(p.UART1, UartConfig::default().with_baudrate(19200))
            .expect("should be able to construct Uart1")
            .with_rx(pins.uart1_rx)
            .with_tx(pins.uart1_tx)
            .into_async();

        let i2c0 = I2c::new(p.I2C0, I2cConfig::default())
            .expect("should be able to construct I2c0")
            .with_scl(pins.i2c0_scl)
            .with_sda(pins.i2c0_sda)
            .into_async();

        let i2c1 = I2c::new(p.I2C1, I2cConfig::default())
            .expect("should be able to construc I2c1")
            .with_scl(pins.i2c1_scl)
            .with_sda(pins.i2c1_sda);

        let pwm = PwmController::new(p.LEDC, pins.pwm, Duty::Duty10Bit);

        #[cfg(feature = "esp32s3")]
        let spi = Spi::new(
            p.SPI2,
            SpiConfig::default()
                .with_frequency(Rate::from_khz(400))
                .with_mode(SpiMode::_0),
        )
        .expect("should be able to construct Spi")
        .with_sck(pins.spi_sck)
        .with_miso(pins.spi_miso)
        .with_mosi(pins.spi_mosi)
        .into_async();

        #[cfg(feature = "esp32s3")]
        let spi_cs = Output::new(pins.spi_cs, Level::High, OutputConfig::default());

        Self {
            uart0: Some(uart0),
            uart1: Some(uart1),
            i2c0: Some(i2c0),
            i2c1: Some(i2c1),
            pwm: Some(pwm),
            #[cfg(feature = "esp32s3")]
            spi: Some(spi),
            #[cfg(feature = "esp32s3")]
            spi_cs: Some(spi_cs),
        }
    }

    pub const fn take_uart0(&mut self) -> Option<Uart<'static, Async>> {
        self.uart0.take()
    }

    pub const fn take_uart1(&mut self) -> Option<Uart<'static, Async>> {
        self.uart1.take()
    }

    pub const fn take_i2c0(&mut self) -> Option<I2c<'static, Async>> {
        self.i2c0.take()
    }

    pub const fn take_i2c1(&mut self) -> Option<I2c<'static, Blocking>> {
        self.i2c1.take()
    }

    pub const fn take_pwm(&mut self) -> Option<PwmController<'static>> {
        self.pwm.take()
    }

    #[cfg(feature = "esp32s3")]
    pub const fn take_spi(&mut self) -> Option<Spi<'static, Async>> {
        self.spi.take()
    }

    #[cfg(feature = "esp32s3")]
    pub const fn take_spi_cs(&mut self) -> Option<Output<'static>> {
        self.spi_cs.take()
    }
}

#[cfg(feature = "esp32")]
struct PinSet {
    uart0_rx: GPIO16<'static>,
    uart0_tx: GPIO17<'static>,
    uart1_rx: GPIO3<'static>,
    uart1_tx: GPIO5<'static>,
    i2c0_scl: GPIO21<'static>,
    i2c0_sda: GPIO22<'static>,
    i2c1_scl: GPIO19<'static>,
    i2c1_sda: GPIO18<'static>,
    pwm: GPIO33<'static>,
}

#[cfg(feature = "esp32")]
impl PinSet {
    fn new(p: &Peripherals) -> Self {
        unsafe {
            Self {
                uart0_rx: p.GPIO16.clone_unchecked(),
                uart0_tx: p.GPIO17.clone_unchecked(),
                uart1_rx: p.GPIO3.clone_unchecked(),
                uart1_tx: p.GPIO5.clone_unchecked(),
                i2c0_scl: p.GPIO21.clone_unchecked(),
                i2c0_sda: p.GPIO22.clone_unchecked(),
                i2c1_scl: p.GPIO19.clone_unchecked(),
                i2c1_sda: p.GPIO18.clone_unchecked(),
                pwm: p.GPIO33.clone_unchecked(),
            }
        }
    }
}

#[cfg(feature = "esp32s3")]
struct PinSet {
    uart0_rx: GPIO2<'static>,
    uart0_tx: GPIO1<'static>,
    uart1_rx: GPIO3<'static>,
    uart1_tx: GPIO5<'static>,
    i2c0_scl: GPIO21<'static>,
    i2c0_sda: GPIO42<'static>,
    i2c1_scl: GPIO8<'static>,
    i2c1_sda: GPIO9<'static>,
    pwm: GPIO34<'static>,
    spi_sck: GPIO12<'static>,
    spi_miso: GPIO13<'static>,
    spi_mosi: GPIO11<'static>,
    spi_cs: GPIO10<'static>,
}

#[cfg(feature = "esp32s3")]
impl PinSet {
    fn new(p: &Peripherals) -> Self {
        // SAFETY: PeripheralManager takes p by value, and doesn't
        // use any GPIOs, so they're only used here
        unsafe {
            Self {
                uart0_rx: p.GPIO2.clone_unchecked(),
                uart0_tx: p.GPIO1.clone_unchecked(),
                uart1_rx: p.GPIO3.clone_unchecked(),
                uart1_tx: p.GPIO5.clone_unchecked(),
                i2c0_scl: p.GPIO21.clone_unchecked(),
                i2c0_sda: p.GPIO42.clone_unchecked(),
                i2c1_scl: p.GPIO8.clone_unchecked(),
                i2c1_sda: p.GPIO9.clone_unchecked(),
                pwm: p.GPIO34.clone_unchecked(),
                spi_sck: p.GPIO12.clone_unchecked(),
                spi_miso: p.GPIO13.clone_unchecked(),
                spi_mosi: p.GPIO11.clone_unchecked(),
                spi_cs: p.GPIO10.clone_unchecked(),
            }
        }
    }
}
