//! Before Kel is built: its icon into `kel.exe`, for Explorer, Start and a pinned
//! shortcut (the window sets its own while Kel runs, see `App::update_icon`).
//! Made from the logo's pixels (`src/logo.rs`), home's colors: change a letter
//! there and the exe's icon follows. Also the file's details: "Kel" in Task
//! Manager and in the file's properties.

// The logo's pixels, the same file Kel draws them from. Kel's other parts don't
// come along: `logo.rs` needs none of them.
#[allow(dead_code)]
#[path = "src/logo.rs"]
mod logo;

use std::path::PathBuf;

/// The theme's default text and background (see `theme.rs`): home's, no color.
const LETTERS: u32 = 0xd4d4d4;
const BACK: u32 = 0x1e1e1e;

/// The sizes Windows asks for: 16..48 in lists and the taskbar at 100..300%,
/// 256 for Explorer's big icons.
const SIZES: [usize; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/logo.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("kel.ico");
    std::fs::write(&ico, ico_file()).unwrap();

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(ico.to_str().unwrap())
        .set("FileDescription", "Kel")
        .set("ProductName", "Kel");
    resource.compile().unwrap();
}

/// An `.ico` file: how many pictures and where each one is, then the pictures,
/// each a PNG.
fn ico_file() -> Vec<u8> {
    let pictures: Vec<Vec<u8>> = SIZES.iter().map(|&size| png(size)).collect();
    let mut file = Vec::new();
    // The header: 0, 1 (an icon, not a cursor), how many.
    for n in [0u16, 1, SIZES.len() as u16] {
        file.extend(n.to_le_bytes());
    }
    // An entry a picture, 16 bytes: its size (256 is written 0), no palette,
    // 1 plane, 32 bits a pixel, its length, where it starts.
    let mut at = 6 + 16 * SIZES.len();
    for (&size, picture) in SIZES.iter().zip(&pictures) {
        let side = if size >= 256 { 0 } else { size as u8 };
        file.extend([side, side, 0, 0]);
        file.extend(1u16.to_le_bytes());
        file.extend(32u16.to_le_bytes());
        file.extend((picture.len() as u32).to_le_bytes());
        file.extend((at as u32).to_le_bytes());
        at += picture.len();
    }
    for picture in pictures {
        file.extend(picture);
    }
    file
}

/// The icon `size` pixels square (see `logo::icon`), as a PNG.
fn png(size: usize) -> Vec<u8> {
    let rgba = logo::icon(size, LETTERS, LETTERS, BACK);
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, size as u32, size as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
    bytes
}
