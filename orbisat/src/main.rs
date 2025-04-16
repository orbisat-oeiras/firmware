use embedded_hal::delay::DelayNs;
use orbisat::signal::SmartSignal;
use rppal::{hal, uart};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signal = SmartSignal::new()?;

    let mut uart = uart::Uart::new(19200, uart::Parity::None, 8, 1)?;

    let mut delay = hal::Delay::new();

    loop {
        if signal.has_fired() {
            println!("Signal received, exiting...");
            break;
        }

        // Simulate some work
        uart.write(b"Hello, UART!\n")?;
        uart.drain()?;
        println!("Sent data over UART");
        delay.delay_ms(500);
    }

    Ok(())
}
