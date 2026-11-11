use crate::{login_animation, AppWindow};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::Value;
use slint::{platform::PointerEventButton, ComponentHandle, Model, PhysicalSize};
use std::time::Duration;

fn strips(app: &AppWindow) -> Vec<ElementHandle> {
    ElementHandle::find_by_element_id(app, "LoginModal::flip-strip").collect()
}

#[test]
fn login_flip_projects_contents_with_chromium_perspective_and_supports_interruption() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    login_animation::attach(&app).unwrap();
    app.window().set_size(PhysicalSize::new(1435, 1000));
    app.show().unwrap();
    let login_state = app.global::<crate::LoginState>();
    login_state.set_is_login_open(true);
    mock_elapsed_time(Duration::from_secs(1));
    mock_elapsed_time(Duration::from_millis(20));
    let _ = app.root_element().size();
    let gt: Value =
        serde_json::from_str(include_str!("../tests/login-modal-ground-truth.json")).unwrap();
    login_state.set_login_mode("qr".into());
    mock_elapsed_time(Duration::ZERO);
    mock_elapsed_time(Duration::ZERO);
    assert!(
        login_state.get_login_flip_ready(),
        "must project captured contents, never resize the layout"
    );
    assert_ne!(
        login_state.get_login_front_frame(),
        login_state.get_login_back_frame()
    );
    let front = login_state.get_login_front_frame().to_rgba8().unwrap();
    let back = login_state.get_login_back_frame().to_rgba8().unwrap();
    for (model, frame) in [
        (login_state.get_login_front_strips(), &front),
        (login_state.get_login_back_strips(), &back),
    ] {
        assert_eq!(model.row_count(), 296);
        let uploaded_pixels: u32 = model
            .iter()
            .map(|image| image.size().width * image.size().height)
            .sum();
        assert_eq!(
            uploaded_pixels,
            frame.width() * frame.height(),
            "strips must not upload duplicate full-frame textures"
        );
    }
    assert_eq!(
        front.as_bytes()[3],
        0,
        "offscreen shadow margin must remain transparent"
    );
    assert_ne!(
        front.as_bytes(),
        back.as_bytes(),
        "front and back captures must contain different controls"
    );
    if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, pixels) in [("front", &front), ("back", &back)] {
            image::save_buffer(
                std::path::PathBuf::from(&dir).join(format!("login-{name}.png")),
                pixels.as_bytes(),
                pixels.width(),
                pixels.height(),
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    for direction in ["samples", "reverse"] {
        if direction == "reverse" {
            login_state.set_login_mode("form".into());
            mock_elapsed_time(Duration::ZERO);
            mock_elapsed_time(Duration::ZERO);
        }
        let mut previous = 0;
        for sample in gt["flip"][direction].as_array().unwrap() {
            let ms = sample["tMs"].as_u64().unwrap();
            mock_elapsed_time(Duration::from_millis(ms - previous));
            previous = ms;
            let strips = strips(&app);
            if ms == 0 || ms == 600 {
                assert!(strips.is_empty(), "endpoints must use live controls");
                continue;
            }
            assert_eq!(strips.len(), 296);
            if ms == 210 {
                for item in &strips {
                    assert!(
                        item.size().width < 0.01,
                        "90 degree edge must collapse without a minimum-width flash"
                    );
                }
                continue;
            }
            let mut corners: Vec<(f32, f32)> = sample["corners"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p["x"].as_f64().unwrap() as f32,
                        p["y"].as_f64().unwrap() as f32,
                    )
                })
                .collect();
            corners.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            // The frame includes 96px of transparent shadow margin around the card.
            for (index, right, expected_top, expected_bottom) in [
                (48, false, corners[0], corners[1]),
                (247, true, corners[2], corners[3]),
            ] {
                let item = &strips[index];
                let position = item.absolute_position();
                let size = item.size();
                let x = position.x + if right { size.width } else { 0.0 };
                let top = position.y + size.height * 96.0 / 752.0;
                let bottom = position.y + size.height * 656.0 / 752.0;
                for (name, actual, expected) in [
                    ("x", x, expected_top.0),
                    ("top", top, expected_top.1),
                    ("bottom", bottom, expected_bottom.1),
                ] {
                    assert!(
                        (actual - expected).abs() < 0.65,
                        "{ms}ms strip {index} {name}: {actual} != Chromium {expected}"
                    );
                }
            }
            if let Some(dir) = std::env::var_os("NCM_SLINT_SCREENSHOT_DIR") {
                let dir = std::path::PathBuf::from(dir);
                std::fs::create_dir_all(&dir).unwrap();
                let pixels = app.window().take_snapshot().unwrap();
                image::save_buffer(
                    dir.join(format!("login-flip-{direction}-{ms}.png")),
                    pixels.as_bytes(),
                    pixels.width(),
                    pixels.height(),
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    login_state.set_login_mode("qr".into());
    mock_elapsed_time(Duration::from_secs(1));
    login_state.set_login_mode("form".into());
    mock_elapsed_time(Duration::from_millis(120));
    let before = strips(&app)[48].absolute_position();
    let button = ElementHandle::find_by_element_id(&app, "LoginModal::flip-switch-btn")
        .next()
        .unwrap();
    let pos = button.absolute_position();
    let size = button.size();
    let position = slint::LogicalPosition::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    assert_eq!(
        login_state.get_login_mode(),
        "qr",
        "projected button must reverse in-flight rotation"
    );
    mock_elapsed_time(Duration::ZERO);
    let after = strips(&app)[48].absolute_position();
    assert!(
        (after.x - before.x).abs() < 0.5,
        "reversal must continue from current angle"
    );
    mock_elapsed_time(Duration::from_secs(1));
    assert!(strips(&app).is_empty());
    login_state.set_is_login_open(false);
    mock_elapsed_time(Duration::from_millis(350));
    login_state.set_is_login_open(true);
    mock_elapsed_time(Duration::from_millis(1));
    assert_eq!(login_state.get_login_mode(), "form");
    assert!(
        strips(&app).is_empty(),
        "reopening must not play the previous QR flip backwards"
    );
    app.hide().unwrap();
}
