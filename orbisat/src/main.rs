use orbisat::{dummy::DummySender, signal::SmartSignal, tmtc::SerialPacketSink};
use rppal::uart;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signal = SmartSignal::new()?;

    let uart = uart::Uart::new(19200, uart::Parity::None, 8, 1)?;
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    let mut sink = SerialPacketSink::new(uart, rx);

    let mut sender = DummySender::new(tx);

    tokio::try_join!(sink.steady(signal.clone()), sender.steady(signal.clone()))?;

    Ok(())
}
