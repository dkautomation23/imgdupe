//! Perceptual hashing: turn an image into 64 bits that survive rescaling,
//! recompression and small edits.
//!
//! dHash is used rather than aHash or pHash because it is the best trade for
//! this job: it compares neighbouring pixels, so it ignores overall brightness
//! (the thing that changes when a file is re-exported), it needs no DCT, and it
//! is a handful of arithmetic per pixel.

use image::imageops::FilterType;
use image::DynamicImage;

/// Width is 9 because 9 columns produce 8 comparisons per row.
const W: u32 = 9;
const H: u32 = 8;

/// 64-bit difference hash of an image.
pub fn dhash(image: &DynamicImage) -> u64 {
    // Triangle is a deliberate choice: Nearest keeps JPEG artefacts and makes
    // re-encoded copies look different, Lanczos costs more for no gain here.
    let small = image.resize_exact(W, H, FilterType::Triangle).to_luma8();

    let mut bits = 0u64;
    let mut index = 0;
    for y in 0..H {
        for x in 0..(W - 1) {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            if left > right {
                bits |= 1 << index;
            }
            index += 1;
        }
    }
    bits
}

/// How many bits differ. 0 = same picture, <= 5 = the same picture edited.
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Hex, so a hash can be grepped or pasted into a spreadsheet.
pub fn to_hex(hash: u64) -> String {
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    /// A deterministic test image: diagonal gradient plus a block of colour.
    fn sample(width: u32, height: u32, shift: u8) -> DynamicImage {
        let mut buffer = RgbImage::new(width, height);
        for (x, y, pixel) in buffer.enumerate_pixels_mut() {
            let value = (((x * 255) / width.max(1)) as u8).wrapping_add(((y * 128) / height.max(1)) as u8);
            *pixel = Rgb([value.wrapping_add(shift), value / 2, 255 - value]);
        }
        for x in 0..width / 4 {
            for y in 0..height / 4 {
                buffer.put_pixel(x, y, Rgb([10, 200, 40]));
            }
        }
        DynamicImage::ImageRgb8(buffer)
    }

    #[test]
    fn the_same_image_hashes_the_same() {
        assert_eq!(dhash(&sample(300, 200, 0)), dhash(&sample(300, 200, 0)));
    }

    #[test]
    fn a_resized_copy_keeps_its_hash() {
        // The whole point: a thumbnail must match the original it came from.
        let original = sample(800, 600, 0);
        let resized = original.resize_exact(200, 150, FilterType::Lanczos3);
        assert!(
            distance(dhash(&original), dhash(&resized)) <= 4,
            "distance was {}",
            distance(dhash(&original), dhash(&resized))
        );
    }

    #[test]
    fn a_brightness_shift_barely_moves_the_hash() {
        let a = dhash(&sample(400, 300, 0));
        let b = dhash(&sample(400, 300, 18));
        assert!(distance(a, b) <= 4, "distance was {}", distance(a, b));
    }

    #[test]
    fn a_different_picture_is_far_away() {
        let gradient = sample(400, 300, 0);
        let mut noise = RgbImage::new(400, 300);
        for (x, y, pixel) in noise.enumerate_pixels_mut() {
            let value = ((x * 7 + y * 13) % 255) as u8;
            *pixel = Rgb([255 - value, value, (value / 3) * 2]);
        }
        let distance = distance(dhash(&gradient), dhash(&DynamicImage::ImageRgb8(noise)));
        assert!(distance > 10, "distance was {distance}");
    }

    #[test]
    fn distance_is_symmetric_and_zero_for_equal_hashes() {
        assert_eq!(distance(0xdead_beef_dead_beef, 0xdead_beef_dead_beef), 0);
        assert_eq!(distance(0b1010, 0b0101), 4);
        assert_eq!(distance(0b0101, 0b1010), 4);
    }

    #[test]
    fn hex_is_always_sixteen_characters() {
        assert_eq!(to_hex(0), "0000000000000000");
        assert_eq!(to_hex(u64::MAX), "ffffffffffffffff");
        assert_eq!(to_hex(255).len(), 16);
    }
}
