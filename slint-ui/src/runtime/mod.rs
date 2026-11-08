mod api;
#[cfg(test)]
mod artist_tests;
mod backend;
mod covers;
mod data;
mod desktop;
mod download;
#[cfg(test)]
mod home_tests;
mod images;
mod library;
#[cfg(test)]
mod login_tests;
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod playlist_loading_tests;
#[cfg(test)]
mod tests;
mod view;
#[cfg(test)]
mod window_tests;

use crate::{AppState, AppWindow, SettingsWindow};
pub(crate) use data::Storage;
use slint::ComponentHandle;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub struct ApplicationRuntime {
    sender: tokio::sync::mpsc::UnboundedSender<backend::Command>,
    thread: Option<std::thread::JoinHandle<()>>,
    _timer: slint::Timer,
    app: slint::Weak<AppWindow>,
    controller: Rc<RefCell<view::View>>,
}
impl ApplicationRuntime {
    pub fn attach(
        app: &AppWindow,
        settings: &SettingsWindow,
        storage: data::Storage,
        preferences: data::Preferences,
    ) -> Result<Self, String> {
        Self::attach_with(app, settings, storage, preferences, None)
    }
    fn attach_with(
        app: &AppWindow,
        settings: &SettingsWindow,
        storage: data::Storage,
        preferences: data::Preferences,
        provider: Option<std::sync::Arc<dyn api::MusicApi>>,
    ) -> Result<Self, String> {
        view::initialize(app, &preferences);
        settings.global::<AppState>().set_runtime_enabled(true);
        let (sender, commands) = tokio::sync::mpsc::unbounded_channel();
        let (events, receiver) = std::sync::mpsc::channel();
        let (image_sender, mut image_receiver) = tokio::sync::mpsc::channel(images::QUEUE_CAPACITY);
        let background_preferences = preferences.clone();
        let thread = std::thread::Builder::new()
            .name("ncm-runtime".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build();
                match rt {
                    Ok(rt) => {
                        rt.block_on(backend::run(
                            commands,
                            events,
                            image_sender,
                            storage,
                            background_preferences,
                            provider,
                        ));
                        rt.shutdown_timeout(Duration::from_secs(2));
                    }
                    Err(e) => {
                        let _ = events.send(backend::Event {
                            name: "startup".into(),
                            data: serde_json::Value::Null,
                            token: 0,
                            epoch: 0,
                            error: Some(format!("无法启动运行时: {e}")),
                        });
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let controller = Rc::new(RefCell::new(view::View::new(sender.clone(), preferences)));
        // Component construction can request images while applying a model. Queue
        // these IDs separately to avoid re-entering the borrowed View from Slint.
        let cover_requests = Rc::new(RefCell::new(std::collections::HashMap::<i32, u32>::new()));
        let pending = cover_requests.clone();
        app.global::<AppState>().on_request_cover(move |id, size| {
            let mut pending = pending.borrow_mut();
            let value = pending.entry(id).or_default();
            *value = (*value).max(size.max(1) as u32);
        });
        for state in [app.global::<AppState>(), settings.global::<AppState>()] {
            let weak = app.as_weak();
            let view = controller.clone();
            state.on_runtime_action(move |name, id, arg| {
                if let Some(app) = weak.upgrade() {
                    view.borrow_mut().action(&app, &name, id, &arg);
                }
            });
        }
        let weak = app.as_weak();
        let view = controller.clone();
        app.global::<AppState>().on_seek_requested(move |percent| {
            if weak.upgrade().is_some() {
                view.borrow_mut().seek(percent);
            }
        });
        macro_rules! editor {
            ($callback:ident,$action:literal) => {{
                let weak = app.as_weak();
                let view = controller.clone();
                app.global::<AppState>().$callback(move || {
                    if let Some(app) = weak.upgrade() {
                        view.borrow_mut().editor(&app, $action);
                    }
                });
            }};
        }
        editor!(on_open_create_playlist, "create");
        editor!(on_open_edit_playlist, "edit");
        editor!(on_close_playlist_editor, "close");
        editor!(on_playlist_input_changed, "validate");
        editor!(on_submit_playlist, "submit");
        editor!(on_request_delete_playlist, "delete");
        editor!(on_confirm_playlist_delete, "confirm-delete");
        let weak = app.as_weak();
        let view = controller.clone();
        app.global::<AppState>().on_open_playlist(move |id| {
            if let Some(app) = weak.upgrade() {
                view.borrow_mut().action(&app, "playlist", id, "");
                true
            } else {
                false
            }
        });
        let timer = slint::Timer::default();
        let weak = app.as_weak();
        let timer_controller = controller.clone();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                if let Some(app) = weak.upgrade() {
                    let mut view = timer_controller.borrow_mut();
                    let started = std::time::Instant::now();
                    for event in receiver.try_iter().take(32) {
                        view.apply(&app, event);
                        if started.elapsed() >= Duration::from_millis(4) {
                            break;
                        }
                    }
                    view.apply_images(
                        &app,
                        std::iter::from_fn(|| image_receiver.try_recv().ok())
                            .take(images::UPDATES_PER_TICK),
                    );
                    view.request_covers(cover_requests.borrow_mut().drain());
                    view.save_preferences_if_changed(&app);
                }
            },
        );
        Ok(Self {
            sender,
            thread: Some(thread),
            _timer: timer,
            app: app.as_weak(),
            controller,
        })
    }
}
impl Drop for ApplicationRuntime {
    fn drop(&mut self) {
        self._timer.stop();
        // A renderer change followed immediately by closing the window must
        // reach storage even if the 50ms preference timer hasn't run yet.
        if let Some(app) = self.app.upgrade() {
            self.controller
                .borrow_mut()
                .save_preferences_if_changed(&app);
        }
        let _ = self.sender.send(backend::Command {
            name: "shutdown".into(),
            data: serde_json::Value::Null,
            token: 0,
        });
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
