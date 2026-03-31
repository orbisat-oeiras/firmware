#![no_std]

#[macro_export]
macro_rules! components {
    (
        ($spawner:ident, $ctx:ident) {
            $(
                $name:ident : $ty:ty = ( $($args:expr),* $(,)? );
            )*
        }
    ) => {
        paste::paste! {
            $(
                let $name: $ty = <$ty>::new($($args),*);

                #[embassy_executor::task]
                async fn [<$name _task>](mut c: $ty, mut ctx_handle: ContextHandle<'static>) {
                    c.run(&mut ctx_handle).await.unwrap();
                }

                $spawner.spawn([<$name _task>]($name, $ctx.to_handle().unwrap())).unwrap();
            )*
        }
    };
}
