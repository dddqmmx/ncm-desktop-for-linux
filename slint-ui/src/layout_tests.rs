use crate::{AppState, AppWindow};
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use slint::{platform::WindowEvent, ComponentHandle, LogicalPosition};
use std::time::Duration;

fn element(app: &impl ComponentHandle, id: &str) -> ElementHandle {
    ElementHandle::find_by_element_id(app, id)
        .next()
        .unwrap_or_else(|| panic!("Missing {id}"))
}

fn init() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
}

fn move_pointer(app: &AppWindow, x: f32, y: f32) {
    app.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(x, y),
    });
    let _ = app.root_element().size();
}

#[test]
fn user_bubble_hover_keeps_its_anchor_and_does_not_oscillate() {
    init();
    let app = AppWindow::new().unwrap();
    app.show().unwrap();
    let state = app.global::<AppState>();
    let bubble = element(&app, "SideNavBar::profile-bubble");
    let trigger = element(&app, "SideNavBar::bubble-trigger");
    for logged_in in [true, false] {
        state.set_is_logged_in(logged_in);
        for pointer_y in [940.0, 969.0, 916.0] {
            move_pointer(&app, 400.0, 600.0);
            mock_elapsed_time(Duration::from_millis(600));
            assert!((bubble.size().height - 56.0).abs() < 0.5);
            let bottom = bubble.absolute_position().y + bubble.size().height;
            let trigger_y = trigger.absolute_position().y;
            let mut last_height = 56.0;
            let mut last_y = bubble.absolute_position().y;
            for frame in 0..90 {
                // Real backends hit-test on redraw. Re-send the stationary pointer
                // every frame so changing geometry cannot hide hover regressions.
                move_pointer(&app, 120.0, pointer_y);
                mock_elapsed_time(Duration::from_millis(16));
                let y = bubble.absolute_position().y;
                let height = bubble.size().height;
                let actual_bottom = y + height;
                assert!((bottom - 5.1..=bottom + 0.1).contains(&actual_bottom),
                    "frame {frame}: moving bottom {actual_bottom} escaped its 5px lift range, pointer={pointer_y}");
                assert!(
                    height + 0.2 >= last_height,
                    "hover collapsed at frame {frame}, pointer={pointer_y}"
                );
                assert!(
                    y <= last_y + 0.2,
                    "hover reversed at frame {frame}, pointer={pointer_y}"
                );
                assert!(
                    (trigger_y - 5.1..=trigger_y + 0.1).contains(&trigger.absolute_position().y)
                );
                last_height = height;
                last_y = y;
            }
            assert!((last_height - if logged_in { 200.4 } else { 109.8 }).abs() < 0.5);
            if logged_in {
                let gt: serde_json::Value =
                    serde_json::from_str(include_str!("../tests/sidebar-hover-ground-truth.json"))
                        .unwrap();
                for (key, id) in [
                    ("bubble", "profile-bubble"),
                    ("trigger", "bubble-trigger"),
                    ("menu", "bubble-menu"),
                    ("content", "sidebar-content"),
                ] {
                    let item = element(&app, &format!("SideNavBar::{id}"));
                    let p = item.absolute_position();
                    let size = item.size();
                    for (axis, actual) in [
                        ("x", p.x),
                        ("y", p.y),
                        ("width", size.width),
                        ("height", size.height),
                    ] {
                        let expected = gt["open"][key]["rect"][axis].as_f64().unwrap() as f32;
                        assert!(
                            (actual - expected).abs() < 0.5,
                            "hover {key}.{axis}: Slint {actual} != Electron {expected}"
                        );
                    }
                }
            }
            // Crossing menu children and their gaps must keep the ancestor hovered.
            let settings = element(&app, "SideNavBar::settings-menu-item");
            let p = settings.absolute_position();
            for _ in 0..30 {
                move_pointer(&app, p.x + 40.0, p.y + 15.0);
                mock_elapsed_time(Duration::from_millis(16));
                assert!((bubble.size().height - last_height).abs() < 0.5);
            }
            if logged_in {
                trigger.mock_single_click(slint::platform::PointerEventButton::Left);
            }
            move_pointer(&app, 400.0, 600.0);
            mock_elapsed_time(Duration::from_millis(500));
            assert!((bubble.size().height - 56.0).abs() < 0.5);
        }
    }
}
