#![no_std]

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};

pub trait Component {
    fn run(
        &self,
        recv: Receiver<'static, CriticalSectionRawMutex, Packet, 256>,
    ) -> impl core::future::Future<Output = ()> + Send;
}

