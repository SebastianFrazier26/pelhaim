use serde::Serialize;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const fn hex(rgb: u32) -> Rgba {
        Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
    }

    /// Scales the existing opacity by `a` (clamped to 0..=1).
    pub fn alpha(self, a: f32) -> Rgba {
        let [r, g, b, al] = self.0;
        Rgba([r, g, b, (f32::from(al) * a.clamp(0.0, 1.0)).round() as u8])
    }
}

// f64 throughout so that a channel landing near .5 rounds the same on every libm.
fn to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round().clamp(0.0, 255.0) as u8
}

fn to_oklab(c: Rgba) -> [f64; 3] {
    let [r, g, b] = [to_linear(c.0[0]), to_linear(c.0[1]), to_linear(c.0[2])];
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

fn from_oklab([ll, a, b]: [f64; 3], alpha: u8) -> Rgba {
    let l = (ll + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (ll - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (ll - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    Rgba([
        from_linear(4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s),
        from_linear(-1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s),
        from_linear(-0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s),
        alpha,
    ])
}

/// Scales OKLab lightness by `l` and chroma (a, b) by `chroma`, keeping hue and alpha.
pub fn oklab_mute(c: Rgba, l: f32, chroma: f32) -> Rgba {
    let [ll, a, b] = to_oklab(c);
    let (l, ch) = (f64::from(l), f64::from(chroma));
    from_oklab([ll * l, a * ch, b * ch], c.0[3])
}

pub fn oklab_l(c: Rgba) -> f32 {
    to_oklab(c)[0] as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_opaque_rgb() {
        assert_eq!(Rgba::hex(0x12AB3C), Rgba([0x12, 0xAB, 0x3C, 255]));
    }

    #[test]
    fn alpha_scales_opacity() {
        assert_eq!(
            Rgba::hex(0x102030).alpha(0.5),
            Rgba([0x10, 0x20, 0x30, 128])
        );
        assert_eq!(Rgba::hex(0x102030).alpha(2.0).0[3], 255);
    }

    #[test]
    fn identity_mute_round_trips() {
        for c in [0x0D0B21, 0xA9DADD, 0x75D068, 0xFFFFFF, 0x000000, 0xFD6160] {
            assert_eq!(oklab_mute(Rgba::hex(c), 1.0, 1.0), Rgba::hex(c), "{c:06X}");
        }
    }

    #[test]
    fn mute_keeps_alpha() {
        assert_eq!(oklab_mute(Rgba([200, 10, 10, 77]), 0.9, 0.5).0[3], 77);
    }

    #[test]
    fn oklab_l_endpoints() {
        assert!(oklab_l(Rgba::hex(0x000000)).abs() < 1e-4);
        assert!((oklab_l(Rgba::hex(0xFFFFFF)) - 1.0).abs() < 1e-3);
    }
}
