//! Exercise compositor resize limits, which the headless backend does not enforce.
use crate::{windows::ApplicationWindows, AppWindow, SettingsState};
use i_slint_backend_testing::ElementRoot;
use serde_json::Value;
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    time::Duration,
};

fn niri_query(name: &str) -> Value {
    let result = Command::new("niri")
        .args(["msg", "-j", name])
        .output()
        .unwrap();
    assert!(result.status.success(), "niri query failed: {name}");
    serde_json::from_slice(&result.stdout).unwrap()
}

fn settings_window_id() -> String {
    niri_query("windows")
        .as_array()
        .unwrap()
        .iter()
        .find(|window| {
            window["pid"].as_u64() == Some(std::process::id() as u64)
                && window["title"].as_str() == Some("设置 · NCM Desktop for Linux")
        })
        .expect("test settings window was not mapped")["id"]
        .to_string()
}

fn niri_action(id: &str, action: &str, value: Option<&str>) {
    // Always target the window belonging to this test process, never the user's focus.
    let mut command = Command::new("niri");
    command.args(["msg", "action", action, "--id", id]);
    if let Some(value) = value {
        command.arg(value);
    }
    assert!(
        command.output().unwrap().status.success(),
        "niri action failed: {action}"
    );
}

#[test]
#[ignore = "requires a real niri desktop; resizes only this test's settings window"]
fn real_settings_minimum_size_survives_reopen_and_monitor_changes() {
    let outputs: Vec<_> = niri_query("outputs")
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, output)| output["logical"].is_object())
        .map(|(name, output)| (name.clone(), output["logical"]["scale"].as_f64().unwrap()))
        .collect();
    assert!(!outputs.is_empty());
    let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
    #[cfg(target_os = "linux")]
    slint::winit_030::winit::platform::wayland::EventLoopBuilderExtWayland::with_any_thread(
        &mut builder,
        true,
    );
    crate::rendering::backend_selector()
        .with_winit_event_loop_builder(builder)
        .select()
        .unwrap();
    let app = AppWindow::new().unwrap();
    let windows = ApplicationWindows::attach(&app).unwrap();
    app.show().unwrap();
    let weak = app.as_weak();
    let settings = windows._settings.as_weak();
    let failures = Rc::new(RefCell::new(Vec::new()));
    let recorded = failures.clone();
    let completed = Rc::new(Cell::new(false));
    let finished = completed.clone();
    let timer = slint::Timer::default();
    let mut case = 0;
    let mut phase = 0;
    let mut id = String::new();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(400),
        move || {
            let app = weak.upgrade().unwrap();
            let settings = settings.upgrade().unwrap();
            let (output, scale) = &outputs[case % outputs.len()];
            let cycle = case / outputs.len();
            match phase {
                0 => {
                    app.global::<SettingsState>().invoke_open_settings_window();
                    settings
                        .global::<SettingsState>()
                        .set_settings_tab(if cycle == 2 { "audio" } else { "general" }.into());
                }
                1 => {
                    id = settings_window_id();
                    niri_action(&id, "move-window-to-floating", None);
                    niri_action(&id, "move-window-to-monitor", Some(output));
                }
                2 => {
                    niri_action(&id, "set-window-width", Some("100"));
                    niri_action(&id, "set-window-height", Some("100"));
                }
                3 => {
                    let size = settings.root_element().size();
                    println!(
                        "settings cycle={cycle} output={output} scale={scale}: minimum={size:?}"
                    );
                    if (size.width - 1000.0).abs() > 2.0 || (size.height - 800.0).abs() > 2.0 {
                        recorded
                            .borrow_mut()
                            .push(format!("cycle={cycle} output={output}: {size:?}"));
                    }
                    niri_action(&id, "set-window-width", Some("1200"));
                    niri_action(&id, "set-window-height", Some("900"));
                }
                _ => {
                    let size = settings.root_element().size();
                    if size.width < 1198.0 || size.height < 898.0 {
                        recorded
                            .borrow_mut()
                            .push(format!("could not enlarge: {size:?}"));
                    }
                    case += 1;
                    if case % outputs.len() == 0 {
                        settings.invoke_close_settings();
                    }
                    if case == outputs.len() * 3 {
                        finished.set(true);
                        app.hide().unwrap();
                        slint::quit_event_loop().unwrap();
                    }
                }
            }
            phase = (phase + 1) % 5;
        },
    );
    slint::run_event_loop().unwrap();
    timer.stop();
    assert!(
        completed.get(),
        "window size check ended before completing all cases"
    );
    assert!(
        failures.borrow().is_empty(),
        "minimum size changed: {:?}",
        failures.borrow()
    );
}
