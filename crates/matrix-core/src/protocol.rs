use crate::{Frame, HEIGHT, WIDTH};

pub fn command(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x32, 0xac, id];
    bytes.extend_from_slice(payload);
    bytes
}
pub fn bw(frame: &Frame) -> Option<Vec<u8>> {
    if !frame.is_valid() {
        return None;
    }
    let mut payload = vec![0; (WIDTH * HEIGHT).div_ceil(8)];
    for (index, value) in frame.pixels.iter().enumerate() {
        if *value > 0 {
            payload[index / 8] |= 1 << (index % 8);
        }
    }
    Some(command(0x06, &payload))
}
pub fn greyscale(frame: &Frame) -> Option<Vec<Vec<u8>>> {
    if !frame.is_valid() {
        return None;
    }
    let mut commands = vec![];
    for x in 0..WIDTH {
        let mut payload = vec![x as u8];
        for y in 0..HEIGHT {
            payload.push(frame.pixels[y * WIDTH + x]);
        }
        commands.push(command(0x07, &payload));
    }
    commands.push(command(0x08, &[]));
    Some(commands)
}
pub fn firmware(bytes: [u8; 3]) -> Option<String> {
    fn bcd(value: u8) -> Option<u8> {
        (value >> 4 <= 9 && value & 15 <= 9).then_some((value >> 4) * 10 + (value & 15))
    }
    Some(format!(
        "{}.{:02}{}",
        bcd(bytes[0])?,
        bcd(bytes[1])?,
        if bytes[2] == 0 { "" } else { "-pre" }
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_matches_measured_orientation_bit_order_and_bcd() {
        let mut frame = crate::render_clock(0, 0).unwrap();
        frame.pixels.fill(0);
        frame.pixels[0] = 180;
        frame.pixels[9] = 255;
        let bytes = bw(&frame).unwrap();
        assert_eq!(bytes.len(), 42);
        assert_eq!(&bytes[..5], &[0x32, 0xac, 6, 1, 2]);
        let columns = greyscale(&frame).unwrap();
        assert_eq!(columns.len(), 10);
        assert_eq!(&columns[0][..6], &[0x32, 0xac, 7, 0, 180, 255]);
        assert_eq!(firmware([0, 0x20, 0]).unwrap(), "0.20");
        assert!(firmware([0, 0xff, 0]).is_none());
    }
}
