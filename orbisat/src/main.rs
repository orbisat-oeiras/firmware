use orbisat::signal::SmartSignal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signal = SmartSignal::new()?;

    loop {
        if signal.has_fired() {
            println!("Signal received, exiting...");
            break;
        }

        // Simulate some work
        std::thread::sleep(std::time::Duration::from_secs(1));
        println!("Sent data over UART");
    }

    Ok(())
}
