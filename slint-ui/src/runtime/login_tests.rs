use super::{
    api::{ApiFuture, ApiResponse, MusicApi},
    data::{Preferences, Storage},
    tests::temp,
    ApplicationRuntime,
};
use crate::AppWindow;
use i_slint_backend_testing::{mock_elapsed_time, ElementHandle, ElementRoot};
use serde_json::{json, Value};
use slint::{
    platform::{Key, PointerEventButton, WindowEvent},
    ComponentHandle, PhysicalSize,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) struct LoginApi {
    ui: std::thread::ThreadId,
    qr_image: String,
    calls: Mutex<Vec<(String, Value)>>,
}
impl LoginApi {
    pub(super) fn new() -> Arc<Self> {
        use base64::Engine;
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        Arc::new(Self {
            ui: std::thread::current().id(),
            qr_image: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            ),
            calls: Mutex::new(vec![]),
        })
    }
    pub(super) fn count(&self, operation: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(op, _)| op == operation)
            .count()
    }
}
impl MusicApi for LoginApi {
    fn call(&self, op: &str, params: Value, _: &str) -> ApiFuture<'_> {
        assert_ne!(std::thread::current().id(), self.ui);
        self.calls.lock().unwrap().push((op.into(), params.clone()));
        let body = match op {
            "personalized" | "recommend_resource" => json!({"code":200,"result":[],"recommend":[]}),
            "personalized_newsong" => json!({"code":200,"result":[]}),
            "recommend_songs" => json!({"code":200,"data":{"dailySongs":[]}}),
            "login" if params["password"] == "test-password" => {
                json!({"code":502,"message":"账号或密码错误"})
            }
            "login" => json!({"code":200,"cookie":"mock-session"}),
            "user_account" => json!({"code":200,"profile":{"userId":42,"nickname":"测试账号"}}),
            "user_playlist" => json!({"code":200,"playlist":[]}),
            "likelist" => json!({"code":200,"ids":[]}),
            "login_qr_key" => json!({"code":200,"data":{"unikey":"mock-key"}}),
            "login_qr_create" => json!({"code":200,"data":{"qrimg":self.qr_image}}),
            "login_qr_check" => json!({"code":801}),
            _ => json!({"code":200}),
        };
        let delayed = matches!(
            op,
            "login" | "captcha_sent" | "login_qr_key" | "login_qr_create"
        );
        Box::pin(async move {
            if delayed {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            assert_ne!(std::thread::current().id(), self.ui);
            Ok(ApiResponse {
                status: 200,
                body,
                cookie: vec![],
            })
        })
    }
}
fn settle() {
    mock_elapsed_time(Duration::from_millis(700));
}
fn element(app: &AppWindow, id: &str) -> ElementHandle {
    [
        "LoginModal",
        "LoginForm",
        "LoginQr",
        "LoginPhoneField",
        "LoginCountryCode",
        "LoginSecretField",
        "LoginMethodTabs",
        "LoginRememberChoice",
        "LoginSubmitButton",
    ]
    .into_iter()
    .find_map(|component| {
        ElementHandle::find_by_element_id(app, &format!("{component}::{id}")).next()
    })
    .unwrap_or_else(|| panic!("Missing {id}"))
}
fn click(app: &AppWindow, id: &str) {
    element(app, id).mock_single_click(PointerEventButton::Left);
    settle();
}
fn key(app: &AppWindow, text: slint::SharedString) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    mock_elapsed_time(Duration::from_millis(10));
}
fn until(condition: impl Fn() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
        mock_elapsed_time(Duration::from_millis(60));
    }
    assert!(condition(), "runtime did not reach expected login state");
}
fn geometry(app: &AppWindow, _gt: &Value, variant: &str) {
    use crate::responsive_tests::{assert_disjoint, assert_inside};
    let card = element(app, "login-card");
    assert_inside(&app.root_element(), &card, variant);
    let phone = element(app, "login-phone-container");
    let secret = element(app, "login-input-field");
    let submit = element(app, "login-submit-btn");
    for field in [&phone, &secret, &submit] {
        assert_inside(&card, field, variant);
    }
    assert_disjoint(&phone, &secret, variant);
    assert_disjoint(&secret, &submit, variant);
    assert!((phone.size().width - secret.size().width).abs() < 0.5);
    assert!((phone.size().width - submit.size().width).abs() < 0.5);
}

#[test]
fn login_modal_matches_electron_forms_keyboard_countdown_and_reopen() {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("software".into()),
        },
    )))
    .unwrap();
    let app = AppWindow::new().unwrap();
    let windows = crate::windows::ApplicationWindows::attach(&app).unwrap();
    let path = temp("login-ui");
    let storage = Storage {
        config: path.clone(),
        cache: path.join("cache"),
    };
    let api = LoginApi::new();
    let runtime = ApplicationRuntime::attach_with(
        &app,
        &windows._settings,
        storage.clone(),
        Preferences::default(),
        Some(api.clone()),
    )
    .unwrap();
    app.window().set_size(PhysicalSize::new(1435, 1000));
    app.show().unwrap();
    let login_state = app.global::<crate::LoginState>();
    let account_state = app.global::<crate::AccountState>();
    login_state.set_is_login_open(true);
    settle();
    let _ = app.root_element().size();
    let gt: Value =
        serde_json::from_str(include_str!("../../tests/login-modal-ground-truth.json")).unwrap();
    assert!(!login_state.get_login_remember());
    geometry(&app, &gt, "initial");
    click(&app, "login-submit-btn");
    assert_eq!(login_state.get_login_error(), "请输入手机号和密码");
    assert!(!api
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(op, _)| op == "login"));

    // Edit the previously non-editable country prefix through real keyboard input.
    click(&app, "country-input");
    // Set the known starting value, then verify a typed digit updates the model.
    login_state.set_login_country_code("".into());
    key(&app, "4".into());
    key(&app, "4".into());
    assert_eq!(login_state.get_login_country_code(), "44");
    login_state.set_login_phone("13000000000".into());
    login_state.set_login_password("test-password".into());
    click(&app, "sec-input");
    key(&app, Key::Return.into());
    assert!(login_state.get_login_busy());
    key(&app, Key::Return.into());
    click(&app, "login-submit-btn");
    until(|| login_state.get_login_error() == "账号或密码错误");
    assert_eq!(api.count("login"), 1, "pending login must not submit twice");
    settle();
    assert_eq!(login_state.get_login_password(), "test-password");
    geometry(&app, &gt, "passwordError");
    assert!(api.calls.lock().unwrap().iter().any(|(op, p)| op == "login"
        && p["countrycode"] == "44"
        && p["password"] == "test-password"
        && p["rememberMe"] == false));

    click(&app, "login-sms-tab");
    assert!(login_state.get_login_error().is_empty());
    assert!(login_state.get_login_sms_phone().is_empty());
    assert_eq!(login_state.get_login_sms_country_code(), "86");
    click(&app, "login-code-button");
    assert_eq!(login_state.get_login_error(), "请输入手机号码");
    login_state.set_login_sms_country_code("81".into());
    login_state.set_login_sms_phone("09000000000".into());
    login_state.set_login_code("123456".into());
    click(&app, "login-code-button");
    until(|| login_state.get_login_captcha_seconds() > 0);
    assert!(login_state.get_login_error().is_empty());
    let countdown = login_state.get_login_captcha_seconds();
    assert!((58..=60).contains(&countdown));
    click(&app, "login-code-button");
    assert_eq!(
        api.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(op, _)| op == "captcha_sent")
            .count(),
        1
    );
    mock_elapsed_time(Duration::from_secs(1));
    assert!(login_state.get_login_captcha_seconds() < countdown);
    assert!(api
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(op, p)| op == "captcha_sent" && p["phone"] == "09000000000" && p["ctcode"] == "81"));
    for _ in 0..60 {
        mock_elapsed_time(Duration::from_secs(1));
    }
    assert_eq!(login_state.get_login_captcha_seconds(), 0);
    click(&app, "login-code-button");
    until(|| {
        api.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(op, _)| op == "captcha_sent")
            .count()
            == 2
    });

    click(&app, "login-password-tab");
    assert_eq!(login_state.get_login_phone(), "13000000000");
    assert_eq!(login_state.get_login_password(), "test-password");
    click(&app, "login-qr-btn");
    until(|| login_state.get_login_qr_ready());
    assert_eq!(login_state.get_login_mode(), "qr");
    assert!(
        !login_state.get_login_busy(),
        "QR polling must not disable form submission"
    );
    click(&app, "login-form-btn");
    assert_eq!(login_state.get_login_mode(), "form");
    assert_eq!(login_state.get_login_phone(), "13000000000");
    assert_eq!(login_state.get_login_password(), "test-password");
    click(&app, "remember-label");
    assert!(
        login_state.get_login_remember(),
        "label must toggle checkbox"
    );
    click(&app, "login-checkmark");
    assert!(
        !login_state.get_login_remember(),
        "checkbox must also toggle"
    );
    key(&app, " ".into());
    assert!(
        login_state.get_login_remember(),
        "focused checkbox must support Space"
    );
    key(&app, Key::Escape.into());
    assert!(
        login_state.get_is_login_open(),
        "Electron does not close login with Escape"
    );
    click(&app, "login-close-btn");
    assert!(!login_state.get_is_login_open());
    login_state.set_is_login_open(true);
    settle();
    assert_eq!(login_state.get_login_mode(), "form");
    assert_eq!(login_state.get_login_tab(), "password");
    assert!(!login_state.get_login_remember());
    assert!(
        login_state.get_login_phone().is_empty() && login_state.get_login_sms_phone().is_empty()
    );
    assert!(login_state.get_login_password().is_empty() && login_state.get_login_code().is_empty());
    assert_eq!(login_state.get_login_captcha_seconds(), 0);
    geometry(&app, &gt, "reopened");

    // Code authentication sends the code, never the password from the other form.
    click(&app, "login-sms-tab");
    login_state.set_login_sms_phone("13000000000".into());
    login_state.set_login_code("654321".into());
    click(&app, "code-input");
    key(&app, Key::Return.into());
    until(|| account_state.get_is_logged_in() && !login_state.get_is_login_open());
    until(|| account_state.get_user_nickname() == "测试账号");
    assert!(!path.join("session.json").exists());
    assert!(api
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|(op, p)| op == "login" && p["captcha"] == "654321" && p.get("password").is_none()));
    until(|| {
        api.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(op, _)| op == "configure_xeapi")
    });
    app.hide().unwrap();
    drop(runtime);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn closed_login_ignores_late_captcha_and_authentication_results() {
    use super::{backend::Event, view::View};
    i_slint_backend_testing::init_no_event_loop();
    let app = AppWindow::new().unwrap();
    let account_state = app.global::<crate::AccountState>();
    let login_state = app.global::<crate::LoginState>();
    account_state.set_is_logged_in(false);
    let (sender, mut commands) = tokio::sync::mpsc::unbounded_channel();
    let mut view = View::new(sender, Preferences::default());
    login_state.set_login_sms_phone("13000000000".into());
    view.action(&app, "captcha", -1, "");
    let captcha = commands.try_recv().unwrap();
    assert!(login_state.get_login_captcha_busy());
    view.action(&app, "close-login", -1, "");
    for error in [None, Some("late captcha failure".into())] {
        view.apply(
            &app,
            Event {
                name: "captcha".into(),
                token: captcha.token,
                epoch: 0,
                data: json!({"code":200}),
                error,
            },
        );
        assert_eq!(login_state.get_login_captcha_seconds(), 0);
        assert!(login_state.get_login_error().is_empty());
        assert!(!login_state.get_login_captcha_busy());
    }
    login_state.set_login_phone("13000000000".into());
    login_state.set_login_password("test-only".into());
    view.action(&app, "login", -1, "");
    let login = commands.try_recv().unwrap();
    view.action(&app, "close-login", -1, "");
    view.apply(
        &app,
        Event {
            name: "login".into(),
            token: login.token,
            epoch: 1,
            data: json!({"authenticated":true}),
            error: None,
        },
    );
    assert!(!account_state.get_is_logged_in());
    assert!(!login_state.get_login_busy());
}
