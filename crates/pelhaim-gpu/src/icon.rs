use std::io::Cursor;

use winit::window::Icon;

// winit only applies a window icon on Windows and X11; macOS takes the .app bundle's icon.
const PNG: &[u8] = include_bytes!("../../../assets/icon/png/pelhaim-256.png");

pub fn window_icon() -> Option<Icon> {
    let mut dec = png::Decoder::new(Cursor::new(PNG));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut rd = dec.read_info().ok()?;
    let mut buf = vec![0; rd.output_buffer_size()?];
    let info = rd.next_frame(&mut buf).ok()?;
    if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
        return None;
    }
    buf.truncate(info.buffer_size());
    Icon::from_rgba(buf, info.width, info.height).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icon_decodes() {
        assert!(super::window_icon().is_some());
    }
}
