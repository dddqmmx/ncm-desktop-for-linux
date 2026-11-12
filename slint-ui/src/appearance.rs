use crate::{AuxButtonItem, SettingsState, Theme};
use slint::{Color, ComponentHandle, Global, Model, ModelRc, VecModel};

pub(crate) fn attach<C: ComponentHandle + 'static>(window: &C)
where
    for<'a> SettingsState<'a>: Global<'a, C>,
    for<'a> Theme<'a>: Global<'a, C>,
{
    let weak = window.as_weak();
    window
        .global::<SettingsState>()
        .on_move_fullscreen_aux(move |id, left, index| {
            if let Some(window) = weak.upgrade() {
                move_aux(&window.global::<SettingsState>(), &id, left, index);
            }
        });
    let weak = window.as_weak();
    window
        .global::<SettingsState>()
        .on_toggle_fullscreen_aux(move |id| {
            if let Some(window) = weak.upgrade() {
                let state = window.global::<SettingsState>();
                for model in [
                    state.get_fullscreen_aux_left(),
                    state.get_fullscreen_aux_right(),
                ] {
                    if let Some(index) = model.iter().position(|item| item.id == id) {
                        let mut item = model.row_data(index).unwrap();
                        item.visible = !item.visible;
                        model.set_row_data(index, item);
                        break;
                    }
                }
            }
        });
    let weak = window.as_weak();
    window
        .global::<SettingsState>()
        .on_set_accent_color(move |color| {
            if let Some(window) = weak.upgrade() {
                apply_accent(
                    &window.global::<SettingsState>(),
                    &window.global::<Theme>(),
                    color,
                );
            }
        });
    let weak = window.as_weak();
    window
        .global::<SettingsState>()
        .on_set_accent_hex(move |hex| {
            let Some(color) = parse_hex(&hex) else {
                return false;
            };
            let Some(window) = weak.upgrade() else {
                return false;
            };
            apply_accent(
                &window.global::<SettingsState>(),
                &window.global::<Theme>(),
                color,
            );
            true
        });
}

fn move_aux(state: &SettingsState<'_>, id: &str, target_left: bool, index: i32) {
    let mut left: Vec<_> = state.get_fullscreen_aux_left().iter().collect();
    let mut right: Vec<_> = state.get_fullscreen_aux_right().iter().collect();
    let source_left = left.iter().any(|item| item.id == id);
    let source = if source_left { &mut left } else { &mut right };
    let Some(source_index) = source.iter().position(|item| item.id == id) else {
        return;
    };
    let item = source.remove(source_index);
    let target = if target_left { &mut left } else { &mut right };
    // The UI reports an insertion slot in the original list, before removal.
    let index = index.max(0) as usize;
    let index = if source_left == target_left && source_index < index {
        index - 1
    } else {
        index
    };
    target.insert(index.min(target.len()), item);
    let model = |items: Vec<AuxButtonItem>| ModelRc::new(VecModel::from(items));
    state.set_fullscreen_aux_left(model(left));
    state.set_fullscreen_aux_right(model(right));
}

pub(crate) fn parse_hex(hex: &str) -> Option<Color> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::from_rgb_u8(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

pub(crate) fn apply_accent(state: &SettingsState<'_>, theme: &Theme<'_>, color: Color) {
    let (red, green, blue) = (color.red(), color.green(), color.blue());
    let mix = |channel: u8, target: f32, weight: f32| {
        (channel as f32 + (target - channel as f32) * weight).round() as u8
    };
    // Match Electron's theme.ts, including rounded RGB channels.
    theme.set_theme_color(Color::from_rgb_u8(red, green, blue));
    theme.set_theme_color_soft(Color::from_argb_f32(
        0.12,
        red as f32 / 255.,
        green as f32 / 255.,
        blue as f32 / 255.,
    ));
    theme.set_theme_color_muted(Color::from_argb_f32(
        0.20,
        red as f32 / 255.,
        green as f32 / 255.,
        blue as f32 / 255.,
    ));
    theme.set_theme_color_strong(Color::from_rgb_u8(
        mix(red, 0., 0.18),
        mix(green, 0., 0.18),
        mix(blue, 0., 0.18),
    ));
    theme.set_theme_color_tint(Color::from_rgb_u8(
        mix(red, 255., 0.82),
        mix(green, 255., 0.82),
        mix(blue, 255., 0.82),
    ));
    state.set_setting_accent_hex(format!("#{red:02x}{green:02x}{blue:02x}").into());
}
