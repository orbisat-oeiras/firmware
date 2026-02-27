#![no_std]

pub mod comms;

pub trait Component {
    fn run(&mut self) -> impl core::future::Future<Output = ()> + Send;
}
