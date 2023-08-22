mod lidar;

pub use crate::lidar::D300;
#[cfg(feature = "usb")]
pub use usb_support::*;


#[cfg(feature = "usb")]
#[allow(dead_code)]
mod usb_support {
    extern crate tokio_serial;

    use std::time::Duration;
    use super::*;
    use tokio::io::{BufReader};
    use tokio_serial::{SerialPortBuilderExt, SerialStream};

    impl D300<SerialStream> {
        pub fn from_usb(port: &str, baud: u32, timeout: Duration) -> Result<Self, tokio_serial::Error> {
            let port = tokio_serial::new(port, baud)
                .timeout(timeout)
                .open_native_async()?;

            Ok(Self {
                rdr: BufReader::new(port),
            })
        }
    }
}
