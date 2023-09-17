use std::io::Error;
use std::pin::Pin;
use futures::{Stream, StreamExt};
use futures::future::ready;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

#[derive(Debug)]
pub struct ScanLine {
    pub distance: u16,
    pub intensity: u8,
}

#[derive(Debug)]
pub struct Frame {
    pub header: u8,
    pub message_type: u8,
    pub len: u8,
    pub speed: u16,
    pub start_angle: f64,
    pub data: Vec<AngledScanLine>,
    pub end_angle: f64,
    pub ts: u16,
    pub crc: u8,
}

pub struct D300<R: AsyncRead> {
    pub(crate) rdr: BufReader<R>
}

#[derive(Debug)]
pub struct AngledScanLine {
    pub distance: usize,
    pub intensity: usize,
    pub angle: f64
}

#[allow(dead_code)]
impl<R: AsyncRead + Unpin> D300<R> {
    fn new(reader: R) -> Self {
        Self {
            rdr: BufReader::new(reader)
        }
    }

    async fn read_frame(&mut self) -> Result<Frame, Error> {
        const EXPECTED_HEADER: u8 = 84;

        loop {
            let header = self.rdr.read_u8().await?;
            if header != EXPECTED_HEADER {
                continue;
            }

            let msg_info = self.rdr.read_u8().await?;
            let message_type = msg_info >> 5;
            let len = msg_info & 0x1F;

            let speed = self.rdr.read_u16_le().await?;
            let start_angle = self.rdr.read_u16_le().await? as f64 / 100.0;

            let mut line_buffer = Vec::with_capacity(len as usize);
            for _ in 0..len {
                let distance = self.rdr.read_u16_le().await?;
                let intensity = self.rdr.read_u8().await?;
                line_buffer.push(ScanLine { distance, intensity });
            }

            let end_angle = self.rdr.read_u16_le().await? as f64 / 100.0;

            // TODO: prop. not right!
            let angle_increment = (end_angle-start_angle)/(len-1) as f64;

            let mut data = Vec::with_capacity(len as usize);

            for (i, scanline) in line_buffer.into_iter().enumerate() {
                let interpolated_angle = start_angle + angle_increment * i as f64;
                data.push(AngledScanLine {
                    distance: scanline.distance as usize,
                    angle: interpolated_angle,
                    intensity: scanline.intensity as usize,
                });
            }

            let ts = self.rdr.read_u16_le().await?;
            let crc = self.rdr.read_u8().await?;

            return Ok(Frame {
                header,
                message_type,
                len,
                speed,
                start_angle,
                data,
                end_angle,
                ts,
                crc,
            });
        }
    }

    fn as_frame_stream(&mut self) -> Pin<Box<dyn Stream<Item = Frame> + '_>> {
        Box::pin(futures::stream::unfold(self, |d300| async {
            match d300.read_frame().await {
                Ok(frame) => Some((frame, d300)),
                Err(_) => None
            }
        }))
    }

    fn as_scan_line_stream(&mut self) -> Pin<Box<dyn Stream<Item = AngledScanLine> + '_>> {
        Box::pin(self.as_frame_stream().flat_map(|frame: Frame| {
            futures::stream::iter(frame.data).boxed()
        }))
    }

    fn frame_in(&mut self, rotations: usize) -> Pin<Box<dyn Stream<Item = Vec<AngledScanLine>> + '_>> {
        let mut line_buffer: Vec<AngledScanLine> = Vec::new();
        let mut covered_angle = 0.0;

        Box::pin(self.as_frame_stream().filter_map(move |mut frame: Frame| {
            covered_angle += if frame.start_angle <= frame.end_angle {
                frame.end_angle - frame.start_angle
            } else {
                (360.0 - frame.start_angle) + frame.end_angle
            };

            line_buffer.append(&mut frame.data);

            if covered_angle >= rotations as f64 * 360.0 {
                ready(Some(std::mem::take(&mut line_buffer)))
            } else {
                ready(None)
            }
        }))
    }
}


#[cfg(test)]
mod tests {
    use std::fs;
    use crate::lidar::D300;
    use futures::StreamExt;
    use tokio_test::block_on;

    fn load_bin_file(filename: &str) -> Vec<u8> {
        fs::read(filename).unwrap()
    }

    #[test]
    fn test_real_world() {
        let data = load_bin_file("lidar_output.bin");
        let mut d300 = D300::new(data.as_slice());

        block_on(async move {
            let mut stream = d300.as_scan_line_stream();
            let first = stream.next().await.unwrap();

            assert_eq!(first.angle, 0.1);
            assert_eq!(first.distance, 2803);
        });
    }
}
