#![no_std]

pub mod sweep {
    include!(concat!(env!("OUT_DIR"), "/sweep.rs"));
}

pub mod pwm;

#[macro_export]
macro_rules! components {
    (
        ($spawner:ident, $ctx:ident) {
            $(
                $name:ident : $ty:ty = ( $($args:expr),* $(,)? )$(.expect($msg:expr))?;
            )*
        }
    ) => {
        paste::paste! {
            $(
                let $name: $ty = <$ty>::new($($args),*)$(.expect($msg))?;

                #[embassy_executor::task]
                async fn [<$name _task>](mut c: $ty, mut ctx_handle: orbisat::ContextHandle<'static>) {
                    match orbisat::Component::run(&mut c, &mut ctx_handle).await {
                        Ok(_) => {},
                        Err(e) => {
                            defmt::error!("`run` future for {} failed", stringify!($name));
                        }
                    }
                }

                $spawner
                    .spawn([<$name _task>]($name, $ctx.to_handle().expect("context should be convertible to a handle")))
                    .expect(concat!("task for ", stringify!($name), " should be spawnable"));
            )*
        }
    };
}
