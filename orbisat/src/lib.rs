#![no_std]

pub mod comms;

pub trait Component {
    fn run(&mut self) -> impl Future<Output = ()>;

    fn run_once(&mut self) -> impl Future<Output = ()>;
}
