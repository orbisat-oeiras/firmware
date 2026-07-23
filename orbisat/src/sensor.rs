use orbipacket::{DeviceId, Payload, TimestampError};

use crate::ContextHandle;

pub trait Sensor<T: Into<Payload>> {
    type Error: core::error::Error + From<TimestampError>;

    fn read(&mut self, ctx: &mut ContextHandle<'_>)
    -> impl Future<Output = Result<T, Self::Error>>;

    fn run_once(
        &mut self,
        ctx: &mut ContextHandle<'_>,
        id: DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        async move {
            let reading = self.read(ctx).await?;
            ctx.send_outbound(id, reading.into()).await?;
            ctx.next_tick().await;

            Ok(())
        }
    }
}

pub mod readings {
    use core::fmt::Display;

    use orbipacket::Payload;

    #[derive(Debug)]
    pub struct Temperature(f32);

    impl Temperature {
        pub fn as_f32(&self) -> &f32 {
            &self.0
        }
    }

    impl Display for Temperature {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{} ºC", self.0)
        }
    }

    impl From<Temperature> for Payload {
        fn from(val: Temperature) -> Self {
            Payload::from_f32(val.0)
        }
    }

    impl From<f32> for Temperature {
        fn from(value: f32) -> Self {
            Self(value)
        }
    }

    #[derive(Debug)]
    pub struct Pressure(f32);

    impl Pressure {
        pub fn as_f32(&self) -> &f32 {
            &self.0
        }
    }

    impl Display for Pressure {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{} Pa", self.0)
        }
    }

    impl From<Pressure> for Payload {
        fn from(val: Pressure) -> Self {
            Payload::from_f32(val.0)
        }
    }

    impl From<f32> for Pressure {
        fn from(value: f32) -> Self {
            Self(value)
        }
    }

    #[derive(Debug)]
    pub struct Humidity(f32);

    impl Humidity {
        pub fn as_f32(&self) -> &f32 {
            &self.0
        }
    }

    impl Display for Humidity {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{} %RH", self.0)
        }
    }

    impl From<Humidity> for Payload {
        fn from(val: Humidity) -> Self {
            Payload::from_f32(val.0)
        }
    }

    impl From<f32> for Humidity {
        fn from(value: f32) -> Self {
            Self(value)
        }
    }

    #[derive(Debug)]
    pub struct Acceleration(f32, f32, f32);

    impl Acceleration {
        pub fn new(x: f32, y: f32, z: f32) -> Self {
            Self(x, y, z)
        }

        pub fn x(&self) -> f32 {
            self.0
        }

        pub fn y(&self) -> f32 {
            self.1
        }

        pub fn z(&self) -> f32 {
            self.2
        }
    }

    impl Display for Acceleration {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "({}, {}, {}) m/s^2", self.0, self.1, self.2)
        }
    }

    impl From<Acceleration> for Payload {
        fn from(value: Acceleration) -> Self {
            const SIZE: usize = size_of::<f32>();
            let mut buf = [0; 3 * SIZE];
            buf[0..SIZE].copy_from_slice(&value.0.to_le_bytes());
            buf[SIZE..2 * SIZE].copy_from_slice(&value.1.to_le_bytes());
            buf[2 * SIZE..3 * SIZE].copy_from_slice(&value.2.to_le_bytes());

            // Unwrapping is safe because buf is small enough
            Payload::from_raw_bytes(buf).unwrap()
        }
    }
}
