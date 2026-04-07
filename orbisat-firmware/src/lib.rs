#![no_std]

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
                async fn [<$name _task>](mut c: $ty, mut ctx_handle: ContextHandle<'static>) {
                    c.run(&mut ctx_handle).await.expect(concat!("`run` future for ", stringify!($name), " should not error"));
                }

                $spawner
                    .spawn([<$name _task>]($name, $ctx.to_handle().expect("context should be convertible to a handle")))
                    .expect(concat!("task for ", stringify!($name), " should be spawnable"));
            )*
        }
    };
}
