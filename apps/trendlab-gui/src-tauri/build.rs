fn main() {
    // Keep the repo text-only: generate a tiny placeholder icon if one isn't present.
    // Tauri's Windows resource generation expects `icons/icon.ico` by default.
    #[cfg(windows)]
    {
        use std::{fs, io, path::Path};

        fn ensure_icon() -> io::Result<()> {
            let icon_path = Path::new("icons").join("icon.ico");
            if icon_path.exists() {
                return Ok(());
            }

            fs::create_dir_all(icon_path.parent().unwrap())?;

            // A minimal ICO containing a 1x1 RGBA PNG.
            // ICO header (6) + dir entry (16) + PNG bytes.
            let png: &[u8] = &[
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
                0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
                0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
                0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
                0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
                0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
            ];

            let png_len = png.len() as u32;
            let image_offset = 6u32 + 16u32;

            let mut ico: Vec<u8> = Vec::with_capacity((image_offset as usize) + png.len());
            // ICONDIR
            ico.extend_from_slice(&[0x00, 0x00]); // reserved
            ico.extend_from_slice(&[0x01, 0x00]); // type = 1 (icon)
            ico.extend_from_slice(&[0x01, 0x00]); // count = 1

            // ICONDIRENTRY
            ico.push(0x01); // width
            ico.push(0x01); // height
            ico.push(0x00); // color count
            ico.push(0x00); // reserved
            ico.extend_from_slice(&1u16.to_le_bytes()); // planes
            ico.extend_from_slice(&32u16.to_le_bytes()); // bit count
            ico.extend_from_slice(&png_len.to_le_bytes()); // bytes in resource
            ico.extend_from_slice(&image_offset.to_le_bytes()); // image offset

            // image data (PNG)
            ico.extend_from_slice(png);

            fs::write(icon_path, ico)?;
            Ok(())
        }

        ensure_icon().expect("failed to generate placeholder icons/icon.ico");
    }

    tauri_build::build()
}


