pub async fn open_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "无效的链接")?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("仅允许打开网页链接".into());
    }
    let status = tokio::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("无法打开外部播放器或浏览器".into())
    }
}
pub fn autostart(enabled: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("无法定位用户目录")?;
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(home).join(".config"));
    let path = base.join("autostart/ncm-slint.desktop");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let escaped = exe
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%");
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        super::data::atomic_write(&path,format!("[Desktop Entry]\nType=Application\nName=NCM Slint\nExec=\"{escaped}\"\nTerminal=false\n").as_bytes())
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
