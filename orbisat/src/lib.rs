#![no_std]

pub mod comms;

pub trait Component {
    fn run(&mut self) -> impl Future<Output = ()> {
        async {
            loop {
                self.run_once().await;
            }
        }
    }

    fn run_once(&mut self) -> impl Future<Output = ()>;
}
