//! Kel's icon as a PNG file, for what isn't kel.exe (`build.rs` puts it in that
//! one): Kel.app's `.icns` and Linux's menus. The release workflow runs it.
//!
//!     cargo run --example icon -- kel.png 256
//!     cargo run --example icon -- kel.png 1024 mac
//!
//! `mac`: a margin round it, as macOS' icons have (the body 824 of 1024), so
//! Kel isn't bigger than the icons next to it in the Dock.

use kel_terminal::logo;

/// The theme's default text and background, as in `build.rs`: home's.
const LETTERS: u32 = 0xd4d4d4;
const BACK: u32 = 0x1e1e1e;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, size, rest @ ..] = args.as_slice() else {
        eprintln!("usage: icon <file.png> <size> [mac]");
        std::process::exit(2);
    };
    let size: usize = size.parse().expect("the size: a number");
    let mac = rest.first().is_some_and(|word| word == "mac");
    let body = if mac { size * 824 / 1024 } else { size };
    let picture = logo::icon(body, LETTERS, LETTERS, BACK);
    // The body in the middle of a see-through square.
    let at = (size - body) / 2;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..body {
        let from = y * body * 4;
        let to = ((y + at) * size + at) * 4;
        rgba[to..to + body * 4].copy_from_slice(&picture[from..from + body * 4]);
    }
    let out = std::fs::File::create(file).expect("can't write the file");
    let mut encoder = png::Encoder::new(out, size as u32, size as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
}
