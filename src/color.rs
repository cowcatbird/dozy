pub const NEUTRAL_KELVIN: u16 = 6500;
pub const MIN_KELVIN: u16 = 1200;

/// What the screen should look like: a white point and an overall brightness multiplier.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look {
    pub kelvin: u16,
    pub brightness: f32,
}

impl Look {
    /// Per-channel output multipliers in 0..=1, with 6500 K at full brightness as (1, 1, 1).
    pub fn channel_gains(self) -> [f32; 3] {
        let [r, g, b] = white_point(self.kelvin);
        [r, g, b].map(|c| c * self.brightness)
    }

    /// 0 at 6500 K, 1 at the warmest supported temperature.
    pub fn warmth(self) -> f32 {
        let k = self.kelvin.clamp(MIN_KELVIN, NEUTRAL_KELVIN);
        f32::from(NEUTRAL_KELVIN - k) / f32::from(NEUTRAL_KELVIN - MIN_KELVIN)
    }
}

/// Black-body white point as RGB gains, normalised so that 6500 K maps to (1, 1, 1).
pub fn white_point(kelvin: u16) -> [f32; 3] {
    let raw = blackbody(f64::from(kelvin.clamp(MIN_KELVIN, NEUTRAL_KELVIN)));
    let reference = blackbody(f64::from(NEUTRAL_KELVIN));
    std::array::from_fn(|i| (raw[i] / reference[i]).clamp(0.0, 1.0) as f32)
}

// Tanner Helland's curve fit of the Planckian locus. Accurate to a few percent between 1000 K and 40000 K,
// which is far below what anyone can see on a warm-tinted screen.
fn blackbody(kelvin: f64) -> [f64; 3] {
    let t = kelvin / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2) };
    let g = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [r, g, b].map(|c| c.clamp(0.0, 255.0) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_is_identity() {
        assert_eq!(white_point(NEUTRAL_KELVIN), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn warmer_means_less_blue_then_less_green() {
        let mut prev = white_point(NEUTRAL_KELVIN);
        for k in (MIN_KELVIN..NEUTRAL_KELVIN).rev().step_by(100) {
            let [r, g, b] = white_point(k);
            assert_eq!(r, 1.0, "red stays full at {k} K");
            assert!(b <= g, "blue drops faster than green at {k} K");
            assert!(g <= prev[1] && b <= prev[2], "monotonic at {k} K");
            prev = [r, g, b];
        }
        let [_, g, b] = white_point(2700);
        assert!(g > 0.6 && g < 0.8 && b > 0.3 && b < 0.5, "2700 K is a candle-ish orange: g={g} b={b}");
    }

    #[test]
    fn brightness_scales_every_channel() {
        let gains = Look { kelvin: NEUTRAL_KELVIN, brightness: 0.5 }.channel_gains();
        assert_eq!(gains, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn warmth_spans_zero_to_one() {
        assert_eq!(Look { kelvin: NEUTRAL_KELVIN, brightness: 1.0 }.warmth(), 0.0);
        assert_eq!(Look { kelvin: MIN_KELVIN, brightness: 1.0 }.warmth(), 1.0);
        assert_eq!(Look { kelvin: 100, brightness: 1.0 }.warmth(), 1.0);
    }
}
