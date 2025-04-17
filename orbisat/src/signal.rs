use std::{
    future::Future,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc},
    task::{Context, Poll},
};

use signal_hook::{
    consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM},
    flag,
};

#[derive(Debug, Clone)]
pub struct SmartSignal {
    flag: Arc<AtomicBool>,
}

impl SmartSignal {
    pub fn new() -> Result<Self, std::io::Error> {
        let flag = Arc::new(AtomicBool::new(false));

        for signal in &[SIGINT, SIGTERM, SIGHUP, SIGQUIT] {
            // When terminated by a second term signal, exit with exit code 1.
            // This will do nothing the first time (because `flag` is still false).
            // However, the second time `flag` will have been set to true by the normal
            // `register`, so this will terminate the process with exit code 1.
            // In this way, we can start a graceful shutdown when getting a signal, but
            // if that gets stuck, we can immediately exit by sending a second signal.
            // The order of registering these is important, if you put this one first, it will
            // first arm and then terminate ‒ all in the first round.
            flag::register_conditional_shutdown(*signal, 1, flag.clone())?;
            flag::register(*signal, flag.clone())?;
        }

        Ok(Self { flag })
    }

    pub fn has_fired(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Future for SmartSignal {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.has_fired() {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[macro_export]
macro_rules! cancellable_loop {
    ($ss:expr => $body:block) => {
        tokio::select! {
            _ = {
                // Check, at compile time, that ss is a SmartSignal.
                let _: &SmartSignal = &$ss;
                $ss
            } => anyhow::Ok(()),
            res = async move {
                $body
                // When using a `loop`, this won't ever be reached, but it is needed
                // for the compiler to infer the type of the async block and allow `?`.
                #[allow(unreachable_code)]
                anyhow::Ok(())
            } => res
        }
    };
}

pub use cancellable_loop;
