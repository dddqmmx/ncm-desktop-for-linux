// Color grouping adapted from extract-colors (used by the Electron frontend).
// MIT License
// Copyright (c) 2025-present Damien Doussaud
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use slint::{Color, Rgba8Pixel, SharedPixelBuffer};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CoverPalette {
    pub primary: Color,
    pub secondary: Color,
    pub is_dark: bool,
}

#[derive(Clone, Copy)]
struct Swatch {
    rgb: [u8; 3],
    count: usize,
}
impl Swatch {
    fn hsl(self) -> [f64; 3] {
        let [r, g, b] = self.rgb.map(|channel| channel as f64 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.;
        let delta = max - min;
        if delta == 0. {
            return [0., 0., lightness];
        }
        let saturation = delta
            / if lightness > 0.5 {
                2. - max - min
            } else {
                max + min
            };
        let hue = if max == r {
            (g - b) / delta + if g < b { 6. } else { 0. }
        } else if max == g {
            (b - r) / delta + 2.
        } else {
            (r - g) / delta + 4.
        } / 6.;
        [hue, saturation, lightness]
    }
    fn same_palette(self, other: Self) -> bool {
        let [h1, s1, l1] = self.hsl();
        let [h2, s2, l2] = other.hsl();
        let hue_distance = (h1 - h2)
            .abs()
            .min(((h1 + 0.5) % 1. - (h2 + 0.5) % 1.).abs());
        hue_distance < 1. / 12. && (s1 - s2).abs() < 0.2 && (l1 - l2).abs() < 0.2
    }
    fn power(self, total: usize) -> f64 {
        let [_, saturation, lightness] = self.hsl();
        let intensity = saturation * (0.5 - (0.5 - lightness).abs()) * 2.;
        // Preserve the installed extract-colors ranking; it includes both
        // intensity and sampled area, rather than simply picking average RGB.
        (intensity + 0.1) * (0.9 - self.count as f64 / total as f64)
    }
    fn color(self) -> Color {
        Color::from_rgb_u8(self.rgb[0], self.rgb[1], self.rgb[2])
    }
}

/// Called in the image decoder task, never on the UI thread. At most 30k pixels
/// and 4096 RGB buckets participate, regardless of the artwork resolution.
pub(crate) fn extract(pixels: &SharedPixelBuffer<Rgba8Pixel>) -> Option<CoverPalette> {
    let stride = pixels.as_slice().len().div_ceil(30_000).max(1);
    let mut leaves: BTreeMap<u16, BTreeMap<[u8; 3], usize>> = BTreeMap::new();
    let mut total = 0;
    for pixel in pixels.as_slice().iter().step_by(stride) {
        total += 1;
        if pixel.a <= 250 {
            continue;
        }
        let bucket =
            ((pixel.r as u16 >> 4) << 8) | ((pixel.g as u16 >> 4) << 4) | (pixel.b as u16 >> 4);
        *leaves
            .entry(bucket)
            .or_default()
            .entry([pixel.r, pixel.g, pixel.b])
            .or_default() += 1;
    }
    let mut colors: Vec<Swatch> = leaves
        .values()
        .map(|leaf| {
            let rgb = *leaf
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                .unwrap()
                .0;
            Swatch {
                rgb,
                count: leaf.values().sum(),
            }
        })
        .collect();
    colors.sort_by_key(|color| std::cmp::Reverse(color.count));
    let mut merged = Vec::new();
    while !colors.is_empty() {
        let mut current = colors.remove(0);
        colors.retain(|color| {
            let distance: u16 = current
                .rgb
                .iter()
                .zip(color.rgb)
                .map(|(&a, b)| a.abs_diff(b) as u16)
                .sum();
            if distance as f64 / 765. < 0.22 {
                current.count += color.count;
                false
            } else {
                true
            }
        });
        merged.push(current);
    }
    let mut groups: Vec<Vec<Swatch>> = Vec::new();
    for color in merged {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.iter().all(|&other| color.same_palette(other)))
        {
            group.push(color);
        } else {
            groups.push(vec![color]);
        }
    }
    let mut colors: Vec<Swatch> = groups
        .iter()
        .map(|group| {
            let rgb = std::array::from_fn(|channel| {
                (group
                    .iter()
                    .map(|color| color.rgb[channel] as usize)
                    .sum::<usize>() as f64
                    / group.len() as f64)
                    .round() as u8
            });
            Swatch {
                rgb,
                count: group.iter().map(|color| color.count).sum(),
            }
        })
        .collect();
    colors.sort_by(|a, b| b.power(total).total_cmp(&a.power(total)));
    let primary = colors.first()?.color();
    let secondary = colors.get(1).unwrap_or(&colors[0]).color();
    // colord.isDark(): weighted RGB brightness < 0.5, independent of app theme.
    let brightness =
        299 * primary.red() as u32 + 587 * primary.green() as u32 + 114 * primary.blue() as u32;
    Some(CoverPalette {
        primary,
        secondary,
        is_dark: brightness < 127_500,
    })
}

pub(crate) fn apply(view: &crate::PlayerViewState<'_>, palette: Option<CoverPalette>) {
    if let Some(palette) = palette {
        view.set_fullscreen_bg_primary(palette.primary);
        view.set_fullscreen_bg_secondary(palette.secondary);
        view.set_fullscreen_is_dark(palette.is_dark);
    }
    view.set_fullscreen_use_accent(palette.is_none());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_uses_electron_ranking_merges_near_colors_and_ignores_transparency() {
        // Ground truth from extractColorsFromImageData({ pixels: 30000 }) in
        // the installed Electron package. Its ranking favors vivid accents.
        let mut pixels = SharedPixelBuffer::new(10, 10);
        for (index, pixel) in pixels.make_mut_slice().iter_mut().enumerate() {
            *pixel = match index {
                0..80 => Rgba8Pixel::new(20, 30, 40, 255),
                80..90 => Rgba8Pixel::new(240, 30, 50, 255),
                _ => Rgba8Pixel::new(30, 180, 220, 255),
            };
        }
        let palette = extract(&pixels).unwrap();
        assert_eq!(palette.primary, Color::from_rgb_u8(240, 30, 50));
        assert_eq!(palette.secondary, Color::from_rgb_u8(30, 180, 220));
        assert!(palette.is_dark);

        let pixels = SharedPixelBuffer::clone_from_slice(
            &[12u8, 34, 56, 255, 13, 35, 57, 255, 255, 255, 255, 250],
            3,
            1,
        );
        let palette = extract(&pixels).unwrap();
        assert_eq!(palette.primary, Color::from_rgb_u8(12, 34, 56));
        assert_eq!(palette.secondary, palette.primary);
        assert!(palette.is_dark);
        assert!(extract(&SharedPixelBuffer::new(0, 0)).is_none());
        assert!(extract(&SharedPixelBuffer::new(10, 10)).is_none());
    }

    #[test]
    fn palette_brightness_matches_colord_for_light_and_dark_artwork() {
        for (channel, is_dark) in [(0, true), (127, true), (128, false), (255, false)] {
            let pixels =
                SharedPixelBuffer::clone_from_slice(&[channel, channel, channel, 255], 1, 1);
            let palette = extract(&pixels).unwrap();
            assert_eq!(palette.is_dark, is_dark);
            assert_eq!(palette.primary, palette.secondary);
        }
    }
}
