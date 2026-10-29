//! Deterministic Framework-sized rendering. No clock, OS, or device I/O.
pub mod config;
pub mod layout;
pub mod power;
pub mod protocol;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const WIDTH: usize = 9;
pub const HEIGHT: usize = 34;
pub const DATA: u8 = 180;

/// Row-major greyscale pixels; (0, 0) is the top left toward the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Frame {
    pub width: u8,
    pub height: u8,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn is_valid(&self) -> bool {
        usize::from(self.width) == WIDTH
            && usize::from(self.height) == HEIGHT
            && self.pixels.len() == WIDTH * HEIGHT
    }
}

const FONT: [[&str; 5]; 10] = [
    ["###", "#.#", "#.#", "#.#", "###"],
    [".#.", "##.", ".#.", ".#.", "###"],
    ["###", "..#", "###", "#..", "###"],
    ["###", "..#", "###", "..#", "###"],
    ["#.#", "#.#", "###", "..#", "..#"],
    ["###", "#..", "###", "..#", "###"],
    ["###", "#..", "###", "#.#", "###"],
    ["###", "..#", "..#", "..#", "..#"],
    ["###", "#.#", "###", "#.#", "###"],
    ["###", "#.#", "###", "..#", "###"],
];

/// Same hour/minute geometry and font as the POC, with remaining rows dark.
/// Invalid wall-clock input is rejected rather than rendered as plausible data.
pub fn render_clock(hour: u8, minute: u8) -> Option<Frame> {
    if hour > 23 || minute > 59 {
        return None;
    }
    let mut frame = Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels: vec![0; WIDTH * HEIGHT],
    };
    for (y, number) in [(1, hour), (7, minute)] {
        for (x, digit) in [(1, number / 10), (5, number % 10)] {
            for (dy, row) in FONT[usize::from(digit)].iter().enumerate() {
                for (dx, pixel) in row.bytes().enumerate() {
                    if pixel == b'#' {
                        frame.pixels[(y + dy) * WIDTH + x + dx] = DATA;
                    }
                }
            }
        }
    }
    Some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midnight_is_zero_padded_with_dark_remaining_rows() {
        let frame = render_clock(0, 0).unwrap();
        assert!(frame.is_valid());
        for y in [1, 7] {
            assert_eq!(
                &frame.pixels[y * WIDTH..(y + 1) * WIDTH],
                &[0, 180, 180, 180, 0, 180, 180, 180, 0]
            );
        }
        assert!(frame.pixels[12 * WIDTH..].iter().all(|p| *p == 0));
        assert_eq!(frame, render_clock(0, 0).unwrap());
    }

    #[test]
    fn all_valid_times_fit_and_only_use_data_intensity() {
        for hour in 0..24 {
            for minute in 0..60 {
                let frame = render_clock(hour, minute).unwrap();
                assert!(frame.is_valid());
                assert!(frame.pixels.iter().all(|p| [0, DATA].contains(p)));
            }
        }
        assert!(render_clock(24, 0).is_none());
        assert!(render_clock(0, 60).is_none());
        assert_ne!(render_clock(12, 34), render_clock(12, 35));
    }

    #[test]
    fn twelve_thirty_four_matches_the_poc_golden_frame() {
        let frame = render_clock(12, 34).unwrap();
        let expected = [
            ".........",
            "..#..###.",
            ".##....#.",
            "..#..###.",
            "..#..#...",
            ".###.###.",
            ".........",
            ".###.#.#.",
            "...#.#.#.",
            ".###.###.",
            "...#...#.",
            ".###...#.",
        ];
        for (y, row) in expected.iter().enumerate() {
            let actual: String = frame.pixels[y * WIDTH..(y + 1) * WIDTH]
                .iter()
                .map(|pixel| if *pixel == DATA { '#' } else { '.' })
                .collect();
            assert_eq!(actual, *row, "row {y}");
        }
        assert!(frame.pixels[12 * WIDTH..].iter().all(|p| *p == 0));
    }
}
