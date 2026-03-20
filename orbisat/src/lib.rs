#![no_std]

use orbipacket::DeviceId;

pub mod comms;

pub trait Component {
    fn id(&self) -> DeviceId;

    fn run(&mut self) -> impl Future<Output = ()> {
        async {
            loop {
                self.run_once().await;
            }
        }
    }

    fn run_once(&mut self) -> impl Future<Output = ()>;
}
