use std::io::Cursor;

use crate::error::{Error, Result};

const WHAT: &str = "a picture file";

const FORMAT_DXT5: u8 = 2;
const FORMAT_PNG: [u8; 2] = [16, 18];
const FORMAT_BGRA: u8 = 28;

const EXTRA_TRUE_SIZE: usize = 3;

const LUMA_CHROMA_NOTE: &[u8] = b"YCoCg";

const LARGEST_SIDE: u32 = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
pub struct Texture<'a> {
    pub width: u32,
    pub height: u32,
    pub format: u8,
    pub true_size: Option<(u32, u32)>,
    pub luma_chroma: bool,
    pub data: &'a [u8],
}

pub fn archive_path(image: &str) -> String {
    format!("panorama/images/{image}_png.vtex_c")
}

pub fn read_texture(file: &[u8]) -> Result<Texture<'_>> {
    let cut_short = || Error::format(WHAT, "the file is cut short");
    let u16_at = |at: usize| file.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(cut_short);
    let u32_at = |at: usize| {
        file.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(cut_short)
    };

    let head_len = u32_at(0)?;
    let blocks_at = 8 + u32_at(8)?;
    let block_count = u32_at(12)?;
    let mut data_block = None;
    for index in 0..block_count.min(64) {
        let entry = blocks_at + 12 * index;
        let name = file.get(entry..entry + 4).ok_or_else(cut_short)?;
        if name == b"DATA" {
            data_block = Some((entry + 4 + u32_at(entry + 4)?, u32_at(entry + 8)?));
        }
    }
    let (at, len) = data_block.ok_or_else(|| Error::format(WHAT, "there is no DATA block"))?;
    if len < 40 {
        return Err(Error::format(WHAT, "the DATA block is too short for a texture"));
    }
    let (width, height) = (u16_at(at + 20)? as u32, u16_at(at + 22)? as u32);

    let extras_at = at + 32 + u32_at(at + 32)?;
    let extra_count = u32_at(at + 36)?;
    let mut true_size = None;
    for index in 0..extra_count.min(64) {
        let entry = extras_at + 12 * index;
        if u32_at(entry)? == EXTRA_TRUE_SIZE && u32_at(entry + 8)? >= 6 {
            let size_at = entry + 4 + u32_at(entry + 4)?;
            let (true_width, true_height) = (u16_at(size_at + 2)? as u32, u16_at(size_at + 4)? as u32);
            let smaller = (1..=width).contains(&true_width) && (1..=height).contains(&true_height);
            true_size = smaller.then_some((true_width, true_height)).filter(|&size| size != (width, height));
        }
    }

    let head = file.get(..head_len).ok_or_else(cut_short)?;
    let luma_chroma = memchr::memmem::find(head, LUMA_CHROMA_NOTE).is_some();

    let texture = Texture {
        width,
        height,
        format: *file.get(at + 26).ok_or_else(cut_short)?,
        true_size,
        luma_chroma,
        data: file.get(head_len..).ok_or_else(cut_short)?,
    };
    let sides = [texture.width, texture.height];
    if sides.contains(&0) || sides.iter().any(|&side| side > LARGEST_SIDE) {
        return Err(Error::format(WHAT, format!("a picture of {} by {} is not believable", texture.width, texture.height)));
    }
    Ok(texture)
}

pub fn decode(file: &[u8]) -> Result<Picture> {
    let texture = read_texture(file)?;
    let picture = if FORMAT_PNG.contains(&texture.format) {
        decode_png(texture.data)?
    } else if texture.format == FORMAT_DXT5 {
        let mut picture = decode_dxt5(&texture)?;
        if texture.luma_chroma {
            to_red_green_blue(&mut picture.rgba);
        }
        picture
    } else if texture.format == FORMAT_BGRA {
        decode_bgra(&texture)?
    } else {
        return Err(Error::format(WHAT, format!("pictures stored in format {} cannot be read yet", texture.format)));
    };
    Ok(match texture.true_size {
        Some((width, height)) => picture.top_left(width, height),
        None => picture,
    })
}

impl Picture {
    pub fn top_left(self, width: u32, height: u32) -> Picture {
        let (width, height) = (width.min(self.width), height.min(self.height));
        if (width, height) == (self.width, self.height) {
            return self;
        }
        let row_len = self.width as usize * 4;
        let rows = self.rgba.chunks_exact(row_len).take(height as usize);
        let rgba = rows.flat_map(|row| &row[..width as usize * 4]).copied().collect();
        Picture { width, height, rgba }
    }

    pub fn fit_within(self, width: u32, height: u32) -> Picture {
        let (width, height) = (width.max(1), height.max(1));
        if (self.width <= width && self.height <= height) || self.width == 0 || self.height == 0 {
            return self;
        }
        let (old_width, old_height) = (self.width as u64, self.height as u64);
        let by_width = width as u64 * old_height <= height as u64 * old_width;
        let (new_width, new_height) = if by_width {
            (width as u64, (old_height * width as u64 / old_width).max(1))
        } else {
            ((old_width * height as u64 / old_height).max(1), height as u64)
        };

        let mut rgba = Vec::with_capacity((new_width * new_height * 4) as usize);
        for y in 0..new_height {
            let (top, bottom) = (y * old_height / new_height, ((y + 1) * old_height / new_height).max(y * old_height / new_height + 1));
            for x in 0..new_width {
                let (left, right) = (x * old_width / new_width, ((x + 1) * old_width / new_width).max(x * old_width / new_width + 1));
                let (mut red, mut green, mut blue, mut alpha) = (0u64, 0u64, 0u64, 0u64);
                for source_y in top..bottom.min(old_height) {
                    let row = (source_y * old_width) as usize * 4;
                    for source_x in left..right.min(old_width) {
                        let pixel = &self.rgba[row + source_x as usize * 4..][..4];
                        let opacity = pixel[3] as u64;
                        red += pixel[0] as u64 * opacity;
                        green += pixel[1] as u64 * opacity;
                        blue += pixel[2] as u64 * opacity;
                        alpha += opacity;
                    }
                }
                let count = (bottom.min(old_height) - top) * (right.min(old_width) - left);
                match alpha {
                    0 => rgba.extend_from_slice(&[0, 0, 0, 0]),
                    _ => rgba.extend_from_slice(&[(red / alpha) as u8, (green / alpha) as u8, (blue / alpha) as u8, (alpha / count) as u8]),
                }
            }
        }
        Picture { width: new_width as u32, height: new_height as u32, rgba }
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("writing to memory cannot fail");
        writer.write_image_data(&self.rgba).expect("the pixels fit the size");
        writer.finish().expect("writing to memory cannot fail");
        out
    }
}

fn decode_png(data: &[u8]) -> Result<Picture> {
    let bad = |error: png::DecodingError| Error::format(WHAT, format!("its PNG data cannot be read: {error}"));
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let size = reader.output_buffer_size().ok_or_else(|| Error::format(WHAT, "its PNG data is too large"))?;
    let mut pixels = vec![0; size];
    let frame = reader.next_frame(&mut pixels).map_err(bad)?;
    pixels.truncate(frame.buffer_size());

    let rgba = match frame.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels.as_chunks().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.as_chunks().0.iter().flat_map(|&[gray, alpha]| [gray, gray, gray, alpha]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&gray| [gray, gray, gray, 255]).collect(),
        png::ColorType::Indexed => return Err(Error::format(WHAT, "its PNG data kept a palette")),
    };
    if rgba.len() != frame.width as usize * frame.height as usize * 4 {
        return Err(Error::format(WHAT, "its PNG data does not fit its size"));
    }
    Ok(Picture { width: frame.width, height: frame.height, rgba })
}

fn to_red_green_blue(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let scale = (pixel[2] as i32 >> 3) + 1;
        let orange = (pixel[0] as i32 - 128) / scale;
        let green = (pixel[1] as i32 - 128) / scale;
        let brightness = pixel[3] as i32;
        let channel = |value: i32| value.clamp(0, 255) as u8;
        *pixel = [channel(brightness + orange - green), channel(brightness + green), channel(brightness - orange - green), 255];
    }
}

fn decode_bgra(texture: &Texture<'_>) -> Result<Picture> {
    let needed = texture.width as usize * texture.height as usize * 4;
    let start = texture.data.len().checked_sub(needed).ok_or_else(|| Error::format(WHAT, "there is too little pixel data"))?;
    let rgba = texture.data[start..].as_chunks().0.iter().flat_map(|&[blue, green, red, alpha]| [red, green, blue, alpha]).collect();
    Ok(Picture { width: texture.width, height: texture.height, rgba })
}

fn decode_dxt5(texture: &Texture<'_>) -> Result<Picture> {
    let (width, height) = (texture.width as usize, texture.height as usize);
    let (blocks_wide, blocks_high) = (width.div_ceil(4), height.div_ceil(4));
    let needed = blocks_wide * blocks_high * 16;
    let start = texture.data.len().checked_sub(needed).ok_or_else(|| Error::format(WHAT, "there is too little pixel data"))?;
    let blocks = &texture.data[start..];

    let mut rgba = vec![0; width * height * 4];
    for (index, block) in blocks.as_chunks::<16>().0.iter().enumerate() {
        let (block_x, block_y) = (index % blocks_wide * 4, index / blocks_wide * 4);
        let alphas = alpha_levels(block[0], block[1]);
        let alpha_bits = u64::from_le_bytes([block[2], block[3], block[4], block[5], block[6], block[7], 0, 0]);
        let colors = color_levels(u16::from_le_bytes([block[8], block[9]]), u16::from_le_bytes([block[10], block[11]]));
        let color_bits = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
        for pixel in 0..16 {
            let (x, y) = (block_x + pixel % 4, block_y + pixel / 4);
            if x >= width || y >= height {
                continue;
            }
            let [red, green, blue] = colors[(color_bits >> (2 * pixel)) as usize & 3];
            let alpha = alphas[(alpha_bits >> (3 * pixel)) as usize & 7];
            rgba[(y * width + x) * 4..][..4].copy_from_slice(&[red, green, blue, alpha]);
        }
    }
    Ok(Picture { width: texture.width, height: texture.height, rgba })
}

fn alpha_levels(first: u8, second: u8) -> [u8; 8] {
    let (a, b) = (first as u32, second as u32);
    let mut levels = [first, second, 0, 0, 0, 0, 0, 255];
    if first > second {
        for step in 1..7 {
            levels[step as usize + 1] = (((7 - step) * a + step * b) / 7) as u8;
        }
    } else {
        for step in 1..5 {
            levels[step as usize + 1] = (((5 - step) * a + step * b) / 5) as u8;
        }
    }
    levels
}

fn color_levels(first: u16, second: u16) -> [[u8; 3]; 4] {
    let unpack = |packed: u16| {
        let (red, green, blue) = ((packed >> 11) & 31, (packed >> 5) & 63, packed & 31);
        [((red << 3) | (red >> 2)) as u8, ((green << 2) | (green >> 4)) as u8, ((blue << 3) | (blue >> 2)) as u8]
    };
    let (a, b) = (unpack(first), unpack(second));
    let mix = |near: [u8; 3], far: [u8; 3]| [0, 1, 2].map(|i| ((2 * near[i] as u32 + far[i] as u32) / 3) as u8);
    [a, b, mix(a, b), mix(b, a)]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn texture_file(width: u16, height: u16, format: u8, data: &[u8]) -> Vec<u8> {
        texture_file_with(width, height, format, None, b"", data)
    }

    fn texture_file_sized(width: u16, height: u16, format: u8, true_size: Option<(u16, u16)>, data: &[u8]) -> Vec<u8> {
        texture_file_with(width, height, format, true_size, b"", data)
    }

    fn texture_file_with(width: u16, height: u16, format: u8, true_size: Option<(u16, u16)>, notes: &[u8], data: &[u8]) -> Vec<u8> {
        let mut header = vec![0u8; 40];
        header[0..2].copy_from_slice(&1u16.to_le_bytes());
        header[20..22].copy_from_slice(&width.to_le_bytes());
        header[22..24].copy_from_slice(&height.to_le_bytes());
        header[24..26].copy_from_slice(&1u16.to_le_bytes());
        header[26] = format;
        header[27] = 1;
        if let Some((true_width, true_height)) = true_size {
            header[32..36].copy_from_slice(&8u32.to_le_bytes());
            header[36..40].copy_from_slice(&1u32.to_le_bytes());
            header.extend_from_slice(&3u32.to_le_bytes());
            header.extend_from_slice(&8u32.to_le_bytes());
            header.extend_from_slice(&6u32.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&true_width.to_le_bytes());
            header.extend_from_slice(&true_height.to_le_bytes());
        }

        let mut file = Vec::new();
        file.extend_from_slice(&((28 + header.len() + notes.len()) as u32).to_le_bytes());
        file.extend_from_slice(&12u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&1u32.to_le_bytes());
        file.extend_from_slice(b"DATA");
        file.extend_from_slice(&8u32.to_le_bytes());
        file.extend_from_slice(&(header.len() as u32).to_le_bytes());
        file.extend_from_slice(&header);
        file.extend_from_slice(notes);
        file.extend_from_slice(data);
        file
    }

    pub(crate) fn png_texture(width: u16, height: u16, color: [u8; 4]) -> Vec<u8> {
        let picture = Picture { width: width as u32, height: height as u32, rgba: color.repeat(width as usize * height as usize) };
        texture_file(width, height, 16, &picture.to_png())
    }

    #[test]
    fn names_the_archive_path_of_a_picture() {
        assert_eq!(archive_path("econ/items/axe/x"), "panorama/images/econ/items/axe/x_png.vtex_c");
    }

    #[test]
    fn reads_a_picture_stored_as_png() {
        let picture = Picture { width: 2, height: 2, rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 9, 8, 7, 6] };
        for format in FORMAT_PNG {
            let file = texture_file(2, 2, format, &picture.to_png());
            let texture = read_texture(&file).unwrap();
            assert_eq!((texture.width, texture.height, texture.format), (2, 2, format));
            assert!(texture.data.starts_with(&[0x89, b'P', b'N', b'G']));
            assert_eq!(decode(&file).unwrap(), picture);
        }
    }

    #[test]
    fn png_without_opacity_or_color_becomes_four_bytes_a_pixel() {
        let encode = |color: png::ColorType, pixels: &[u8]| {
            let mut out = Vec::new();
            let mut encoder = png::Encoder::new(&mut out, 2, 1);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
            writer.finish().unwrap();
            out
        };
        let rgb = decode(&texture_file(2, 1, 16, &encode(png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6]))).unwrap();
        assert_eq!(rgb.rgba, [1, 2, 3, 255, 4, 5, 6, 255]);
        let gray = decode(&texture_file(2, 1, 16, &encode(png::ColorType::Grayscale, &[7, 9]))).unwrap();
        assert_eq!(gray.rgba, [7, 7, 7, 255, 9, 9, 9, 255]);
        let gray_alpha = decode(&texture_file(2, 1, 16, &encode(png::ColorType::GrayscaleAlpha, &[7, 1, 9, 2]))).unwrap();
        assert_eq!(gray_alpha.rgba, [7, 7, 7, 1, 9, 9, 9, 2]);
    }

    #[test]
    fn reads_a_picture_stored_as_dxt5_blocks() {
        let alpha_bits: u64 = 1 << 3 | 2 << 6;
        let color_bits: u32 = 1 << 2 | 2 << 4 | 3 << 6;
        let mut block = vec![255, 0];
        block.extend_from_slice(&alpha_bits.to_le_bytes()[..6]);
        block.extend_from_slice(&0xF800u16.to_le_bytes());
        block.extend_from_slice(&0x001Fu16.to_le_bytes());
        block.extend_from_slice(&color_bits.to_le_bytes());

        let picture = decode(&texture_file(4, 4, FORMAT_DXT5, &block)).unwrap();
        assert_eq!((picture.width, picture.height), (4, 4));
        assert_eq!(picture.rgba[0..4], [255, 0, 0, 255], "color 0, full opacity");
        assert_eq!(picture.rgba[4..8], [0, 0, 255, 0], "color 1, no opacity");
        assert_eq!(picture.rgba[8..12], [170, 0, 85, 218], "two thirds red, 6/7 opacity");
        assert_eq!(picture.rgba[12..16], [85, 0, 170, 255], "two thirds blue");
        assert_eq!(picture.rgba[16..20], [255, 0, 0, 255], "the second row starts with color 0");

        let two_blocks = [vec![0u8; 16], block.clone(), block].concat();
        let narrow = decode(&texture_file(6, 3, FORMAT_DXT5, &two_blocks)).unwrap();
        assert_eq!(narrow.rgba.len(), 6 * 3 * 4);
        assert_eq!(narrow.rgba[0..4], [255, 0, 0, 255]);
        assert_eq!(narrow.rgba[16..20], [255, 0, 0, 255], "the fifth pixel is the second block's first");
        assert_eq!(narrow.rgba[20..24], [0, 0, 255, 0]);
    }

    #[test]
    fn reads_a_picture_stored_as_plain_bgra() {
        let file = texture_file(2, 1, FORMAT_BGRA, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(decode(&file).unwrap().rgba, [3, 2, 1, 4, 7, 6, 5, 8]);
        assert!(decode(&texture_file(2, 2, FORMAT_BGRA, &[0; 8])).is_err(), "too little data");
    }

    #[test]
    fn a_padded_texture_gives_only_the_picture_inside_it() {
        let data: Vec<u8> = (0..32).collect();
        let file = texture_file_sized(4, 2, FORMAT_BGRA, Some((3, 1)), &data);
        let texture = read_texture(&file).unwrap();
        assert_eq!((texture.width, texture.height, texture.true_size), (4, 2, Some((3, 1))));
        let picture = decode(&file).unwrap();
        assert_eq!((picture.width, picture.height), (3, 1));
        assert_eq!(picture.rgba, [2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11]);

        for odd in [(4, 2), (5, 2), (0, 1)] {
            let file = texture_file_sized(4, 2, FORMAT_BGRA, Some(odd), &data);
            assert_eq!(read_texture(&file).unwrap().true_size, None, "{odd:?}");
            assert_eq!(decode(&file).unwrap().rgba.len(), 32);
        }
    }

    #[test]
    fn pictures_are_made_smaller_by_averaging() {
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
        let halves = Picture { width: 4, height: 2, rgba: [red, red, blue, blue, red, red, blue, blue].concat() };
        let small = halves.fit_within(2, 2);
        assert_eq!((small.width, small.height), (2, 1));
        assert_eq!(small.rgba, [255, 0, 0, 255, 0, 0, 255, 255]);

        let corner = Picture { width: 2, height: 2, rgba: [[255, 255, 255, 255], [0; 4], [0; 4], [0; 4]].concat() };
        assert_eq!(corner.fit_within(1, 1).rgba, [255, 255, 255, 63]);

        let fits = Picture { width: 2, height: 2, rgba: vec![9; 16] };
        assert_eq!(fits.clone().fit_within(2, 5), fits);
        let tall = Picture { width: 112, height: 170, rgba: vec![7; 112 * 170 * 4] }.fit_within(128, 128);
        assert_eq!((tall.width, tall.height), (84, 128));
        assert!(tall.rgba.iter().all(|&byte| byte == 7));
        let wide = Picture { width: 1920, height: 1080, rgba: vec![7; 1920 * 1080 * 4] }.fit_within(256, 256);
        assert_eq!((wide.width, wide.height, wide.rgba.len()), (256, 144, 256 * 144 * 4));
        let sliver = Picture { width: 1000, height: 2, rgba: vec![7; 8000] }.fit_within(10, 0);
        assert_eq!((sliver.width, sliver.height), (10, 1), "never smaller than one pixel");
    }

    #[test]
    fn brightness_and_color_differences_become_red_green_and_blue() {
        let block = |scale_bits: u16| {
            let color = (18u16 << 11) | (27 << 5) | scale_bits;
            let mut block = vec![100, 100, 0, 0, 0, 0, 0, 0];
            block.extend_from_slice(&color.to_le_bytes());
            block.extend_from_slice(&color.to_le_bytes());
            block.extend_from_slice(&0u32.to_le_bytes());
            block
        };
        let note = b"CompileTexture\0Texture Compiler Version Image YCoCg Conversion\0";
        let decoded = |scale_bits: u16, notes: &[u8]| {
            let file = texture_file_with(4, 4, FORMAT_DXT5, None, notes, &block(scale_bits));
            (read_texture(&file).unwrap().luma_chroma, decode(&file).unwrap().rgba[..4].to_vec())
        };

        assert_eq!(decoded(0, note), (true, vec![139, 81, 99, 255]), "100 + 20 + 19, 100 - 19, 100 - 20 + 19");
        assert_eq!(decoded(1, note), (true, vec![119, 91, 99, 255]), "the differences are halved first");
        assert_eq!(decoded(0, b"CompileTexture\0Texture Compiler Version Mip None\0"), (false, vec![148, 109, 0, 100]), "stored plainly");
        assert!(decoded(0, b"Texture Compiler VersW\x01 YCoCg Conv`\x02").0);

        let png = Picture { width: 1, height: 1, rgba: vec![148, 109, 0, 100] };
        let file = texture_file_with(1, 1, 18, None, note, &png.to_png());
        assert_eq!(decode(&file).unwrap(), png);
    }

    #[test]
    fn opacity_levels_follow_the_two_rules() {
        assert_eq!(alpha_levels(255, 0), [255, 0, 218, 182, 145, 109, 72, 36]);
        assert_eq!(alpha_levels(0, 255), [0, 255, 51, 102, 153, 204, 0, 255]);
        assert_eq!(alpha_levels(10, 10), [10, 10, 10, 10, 10, 10, 0, 255]);
    }

    #[test]
    fn files_that_are_not_pictures_are_refused() {
        assert!(read_texture(b"").is_err());
        assert!(read_texture(&[0; 16]).is_err(), "no blocks");
        let good = png_texture(2, 2, [1, 2, 3, 4]);
        assert!(decode(&good).is_ok());
        assert!(read_texture(&good[..40]).is_err(), "cut short");
        assert!(decode(&texture_file(2, 2, 16, b"not a png")).is_err());
        assert!(decode(&texture_file(2, 2, 20, &[0; 64])).is_err(), "a format that is not read yet");
        assert!(decode(&texture_file(8, 8, FORMAT_DXT5, &[0; 16])).is_err(), "too little data");
        assert!(read_texture(&texture_file(0, 2, 16, b"")).is_err(), "no width");
        assert!(read_texture(&texture_file(9000, 2, 16, b"")).is_err(), "too wide");
    }
}
