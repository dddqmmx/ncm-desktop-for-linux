//! Capture the live Slint card once per flip and project its texture strips.
//! Network/image decoding remains in runtime::images. This is ordinary UI
//! rendering into a small offscreen window, never a per-frame screenshot.
use crate::{AppWindow, LoginCaptureWindow, Theme};
use slint::{ComponentHandle, LogicalSize, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::{rc::Rc, time::Duration};

fn strips(pixels: &SharedPixelBuffer<Rgba8Pixel>) -> ModelRc<slint::Image> {
    let width = pixels.width() as usize;
    let mut images = Vec::with_capacity(296);
    for i in 0..296 {
        let left = i * width / 296;
        let right = (i + 1) * width / 296;
        let mut strip =
            SharedPixelBuffer::<Rgba8Pixel>::new((right - left) as u32, pixels.height());
        for (src, dst) in pixels
            .as_bytes()
            .chunks_exact(width * 4)
            .zip(strip.make_mut_bytes().chunks_exact_mut((right - left) * 4))
        {
            dst.copy_from_slice(&src[left * 4..right * 4]);
        }
        images.push(slint::Image::from_rgba8(strip));
    }
    Rc::new(VecModel::from(images)).into()
}

pub fn attach(app: &AppWindow) -> Result<(), slint::PlatformError> {
    let weak = app.as_weak();
    let capture = LoginCaptureWindow::new()?;
    let timer = Rc::new(slint::Timer::default());
    app.global::<crate::LoginState>().on_prepare_login_flip(move || {
        let weak = weak.clone();
        let capture = capture.clone_strong();
        // Property change callbacks may run while the GPU is rendering. Defer
        // offscreen rendering until that render pass has returned.
        timer.start(slint::TimerMode::SingleShot, Duration::ZERO, move || {
            let Some(app) = weak.upgrade() else { return };
            let result = (|| -> Result<(), slint::PlatformError> {
                let source_login_state = app.global::<crate::LoginState>();
                let source_runtime_state = app.global::<crate::RuntimeState>();
                let target_login_state = capture.global::<crate::LoginState>();
                let target_runtime_state = capture.global::<crate::RuntimeState>();
                crate::windows::sync_theme(app.global::<Theme>(), capture.global::<Theme>(), None);
                macro_rules! copy {
                ($($get:ident => $set:ident),* $(,)?) => { $(target_login_state.$set(source_login_state.$get());)* };
            }
                target_runtime_state.set_runtime_enabled(source_runtime_state.get_runtime_enabled());
                copy!(
                    get_login_tab => set_login_tab,
                    get_login_country_code => set_login_country_code,
                    get_login_sms_country_code => set_login_sms_country_code,
                    get_login_phone => set_login_phone,
                    get_login_sms_phone => set_login_sms_phone,
                    get_login_password => set_login_password,
                    get_login_code => set_login_code,
                    get_login_error => set_login_error,
                    get_login_remember => set_login_remember,
                    get_login_busy => set_login_busy,
                    get_login_captcha_busy => set_login_captcha_busy,
                    get_login_captcha_seconds => set_login_captcha_seconds,
                    get_login_qr_image => set_login_qr_image,
                    get_login_qr_ready => set_login_qr_ready,
                    get_login_qr_message => set_login_qr_message,
                );
                capture
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                        scale_factor: app.window().scale_factor(),
                    });
                capture.window().set_size(LogicalSize::new(592.0, 752.0));
                capture.set_show_back(false);
                let front = capture.window().take_snapshot()?;
                capture.set_show_back(true);
                let back = capture.window().take_snapshot()?;
                target_login_state.set_login_password("".into());
                target_login_state.set_login_code("".into());
                // Programmatic Slint Images have no GPU cache key. Feeding the
                // full frame to every Image would upload it 296 times (~1GB).
                // Upload disjoint strips instead: total texture bytes stay O(frame).
                source_login_state.set_login_front_strips(strips(&front));
                source_login_state.set_login_back_strips(strips(&back));
                source_login_state.set_login_front_frame(slint::Image::from_rgba8(front));
                source_login_state.set_login_back_frame(slint::Image::from_rgba8(back));
                source_login_state.set_login_render_scale(app.window().scale_factor());
                source_login_state.set_login_flip_ready(true);
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("登录翻转渲染失败: {error}");
            }
        });
    });
    Ok(())
}
