//! One code-native mark for the EXE resource, window and notification area.

pub fn rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut color = [0.0f32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = (x as f32 + (sx as f32 + 0.5) / 4.) / size as f32;
                    let py = (y as f32 + (sy as f32 + 0.5) / 4.) / size as f32;
                    if (px - 0.5).powi(2) + (py - 0.5).powi(2) > 0.46f32.powi(2) {
                        continue;
                    }
                    let arrow = (0.445..=0.555).contains(&px) && (0.23..=0.55).contains(&py)
                        || (0.43..=0.68).contains(&py)
                            && (px - 0.5).abs() <= 0.68 - py
                            && (px - 0.5).abs() <= 0.20
                        || (0.30..=0.70).contains(&px) && (0.74..=0.81).contains(&py);
                    let rgb = if arrow {
                        [255., 255., 255.]
                    } else {
                        [33., 99., 209.]
                    };
                    for c in 0..3 {
                        color[c] += rgb[c];
                    }
                    color[3] += 1.;
                }
            }
            let at = ((y * size + x) * 4) as usize;
            if color[3] > 0. {
                for c in 0..3 {
                    pixels[at + c] = (color[c] / color[3]).round() as u8;
                }
                pixels[at + 3] = (color[3] * 255. / 16.).round() as u8;
            }
        }
    }
    pixels
}

#[allow(dead_code)] // Used by build.rs as well as the explicit icon-export command.
pub fn ico() -> Vec<u8> {
    let sizes = [16u32, 20, 24, 32, 48, 64, 256];
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut images = Vec::new();
    let mut offset = 6 + 16 * sizes.len();
    for size in sizes {
        let rgba = rgba(size);
        let mask_stride = size.div_ceil(32) * 4;
        let bytes = size * size * 4 + mask_stride * size;
        let mut image = Vec::new();
        for value in [40u32, size, size * 2] {
            image.extend_from_slice(&value.to_le_bytes());
        }
        image.extend_from_slice(&1u16.to_le_bytes());
        image.extend_from_slice(&32u16.to_le_bytes());
        for value in [0u32, bytes, 0, 0, 0, 0] {
            image.extend_from_slice(&value.to_le_bytes());
        }
        for y in (0..size).rev() {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                image.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        for y in (0..size).rev() {
            let mut mask = vec![0u8; mask_stride as usize];
            for x in 0..size {
                if rgba[((y * size + x) * 4 + 3) as usize] == 0 {
                    mask[(x / 8) as usize] |= 0x80 >> (x % 8);
                }
            }
            image.extend(mask);
        }
        out.extend_from_slice(&[
            if size == 256 { 0 } else { size as u8 },
            if size == 256 { 0 } else { size as u8 },
            0,
            0,
        ]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += image.len();
        images.push(image);
    }
    for image in images {
        out.extend(image);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    fn windows_decodes_the_embedded_application_icon() {
        use windows_sys::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{DestroyIcon, IMAGE_ICON, LoadImageW},
        };
        for size in [16, 20, 24, 32, 48, 64, 256] {
            let icon = unsafe {
                LoadImageW(
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::without_provenance(1), // Win32 MAKEINTRESOURCEW(1), not a string pointer.
                    IMAGE_ICON,
                    size,
                    size,
                    0,
                )
            };
            assert!(
                !icon.is_null(),
                "Windows could not decode embedded icon group 1"
            );
            unsafe {
                DestroyIcon(icon);
            }
        }
    }
    #[test]
    fn icon_is_legible_and_ico_contains_all_sizes() {
        for size in [16, 20, 24, 32, 48, 64, 256] {
            let pixels = rgba(size);
            assert_eq!(pixels.len(), (size * size * 4) as usize);
            let (rgba_pixels, _) = pixels.as_chunks::<4>();
            assert!(rgba_pixels.contains(&[255, 255, 255, 255]));
            assert!(rgba_pixels.contains(&[33, 99, 209, 255]));
            assert_eq!(pixels[3], 0);
        }
        let bytes = ico();
        assert_eq!(&bytes[..6], &[0, 0, 1, 0, 7, 0]);
        for index in 0..7 {
            let entry = 6 + index * 16;
            let len = u32::from_le_bytes(bytes[entry + 8..entry + 12].try_into().unwrap()) as usize;
            let offset =
                u32::from_le_bytes(bytes[entry + 12..entry + 16].try_into().unwrap()) as usize;
            assert!(offset + len <= bytes.len());
            assert_eq!(
                u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                40
            );
        }
    }
}
