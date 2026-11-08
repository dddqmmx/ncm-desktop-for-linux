use super::{
    api::MusicApi,
    data::{items, text, Song},
};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::io::AsyncWriteExt;
const LEVELS: &[&str] = &[
    "none", "standard", "higher", "exhigh", "lossless", "hires", "jyeffect", "sky", "jymaster",
];
fn rank(v: &Value) -> Option<usize> {
    v.as_str().and_then(|s| LEVELS.iter().position(|l| *l == s))
}
fn bitrate(v: &Value) -> Option<usize> {
    v.as_i64().map(|b| {
        if b >= 999000 {
            4
        } else if b >= 320000 {
            3
        } else if b >= 192000 {
            2
        } else if b >= 128000 {
            1
        } else {
            0
        }
    })
}
pub fn allowed(song: &Value, privilege: &Value, quality: &str) -> Result<(), String> {
    let q = LEVELS
        .iter()
        .position(|l| *l == quality)
        .filter(|q| *q > 0)
        .ok_or("请选择有效音质")?;
    if privilege["st"].as_i64().unwrap_or(0) < 0 || privilege["code"].as_i64().unwrap_or(0) >= 400 {
        return Err("歌曲暂不可下载".into());
    }
    let account = if let Some(levels) = privilege["dlLevels"].as_array() {
        levels.iter().any(|l| l == quality)
    } else {
        let limit = rank(&privilege["dlLevel"])
            .or_else(|| bitrate(&privilege["dl"]))
            .ok_or("未能获取当前账号的下载权限，请重试")?;
        q <= limit
    };
    if !account {
        return Err("当前会员或购买权限不支持此下载音质".into());
    }
    let br = privilege
        .get("downloadMaxbr")
        .or(privilege.get("maxbr"))
        .unwrap_or(&Value::Null);
    let source = rank(&privilege["downloadMaxBrLevel"])
        .or_else(|| rank(&privilege["maxBrLevel"]))
        .or_else(|| {
            if br.as_i64().unwrap_or(0) >= 999000 {
                None
            } else {
                bitrate(br)
            }
        });
    let field = match quality {
        "standard" => Some("l"),
        "exhigh" => Some("h"),
        "lossless" => Some("sq"),
        "hires" => Some("hr"),
        _ => None,
    };
    if source.is_some_and(|s| q > s)
        || field.is_some_and(|f| song.get(f).is_some_and(Value::is_null))
    {
        return Err("歌曲暂无此音质".into());
    }
    Ok(())
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub async fn quality_options(
    api: &dyn MusicApi,
    cookie: &str,
    song: &Song,
) -> Result<Value, String> {
    if song.path.is_some() {
        return Err("本地歌曲已保存在设备上".into());
    }
    let detail = api
        .call("song_download_detail", json!({"ids":song.id}), cookie)
        .await?
        .checked()?
        .body;
    let detail_song = items(&detail["songs"])
        .iter()
        .find(|v| text(&v["id"]) == song.id)
        .ok_or("无法获取歌曲信息")?;
    let privilege = items(&detail["privileges"])
        .iter()
        .find(|v| text(&v["id"]) == song.id)
        .ok_or("无法获取下载权限")?;
    Ok(json!([("standard","标准"),("exhigh","极高"),("lossless","无损"),("hires","Hi-Res"),("jyeffect","高清环绕声"),("sky","沉浸环绕声"),("jymaster","超清母带")].into_iter().map(|(quality,label)|{
        let result=allowed(detail_song,privilege,quality);
        json!({"quality":quality,"label":label,"available":result.is_ok(),"reason":result.err().unwrap_or_default()})
    }).collect::<Vec<_>>()))
}
pub async fn download(
    api: Result<Arc<dyn MusicApi>, String>,
    cookie: &str,
    data: &Value,
) -> Result<Value, String> {
    let api = api?;
    let song: Song = serde_json::from_value(data["song"].clone()).map_err(|e| e.to_string())?;
    let quality = data["quality"].as_str().unwrap_or("lossless");
    let options = quality_options(api.as_ref(), cookie, &song).await?;
    let option = items(&options)
        .iter()
        .find(|o| o["quality"] == quality)
        .ok_or("请选择有效音质")?;
    if option["available"] != true {
        return Err(text(&option["reason"]));
    }
    let body = api
        .call(
            "song_download_url",
            json!({"id":song.id,"level":quality}),
            cookie,
        )
        .await?
        .checked()?
        .body;
    let source = if body["data"].is_array() {
        &body["data"][0]
    } else {
        &body["data"]
    };
    if source["code"].as_i64().unwrap_or(200) != 200
        || (!source["freeTrialInfo"].is_null() && source["freeTrialInfo"] != false)
    {
        return Err("该歌曲当前不可下载完整音源".into());
    }
    if source["level"] != quality {
        return Err("服务返回的音质与所选音质不符，请重新选择".into());
    }
    let url = reqwest::Url::parse(&text(&source["url"])).map_err(|_| "无效的下载地址")?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("无效的下载协议".into());
    }
    let ext = source["type"]
        .as_str()
        .or(source["encodeType"].as_str())
        .filter(|s| ["mp3", "flac", "m4a", "aac", "wav", "ogg", "opus"].contains(s))
        .unwrap_or("mp3");
    let name = format!("{} - {}", song.name, song.artist)
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .take(60)
        .collect::<String>();
    let name = format!(
        "{}.{}",
        name.trim_matches(|c: char| c == '.' || c.is_whitespace()),
        ext
    );
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title("下载歌曲")
        .set_file_name(name)
        .add_filter("音频", &[ext])
        .save_file()
        .await
    else {
        return Ok(json!({"cancelled":true}));
    };
    let target = file.path();
    if target.exists() {
        return Err("文件已存在，请另选文件名以保留原文件".into());
    }
    let tmp = Partial(target.with_extension(format!(
            "ncm-{}-{}.part",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )));
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp.0)
        .await
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "下载连接失败")?
        .error_for_status()
        .map_err(|_| "服务器拒绝下载")?;
    let mut stream = response.bytes_stream();
    let mut size = 0u64;
    while let Some(bytes) = tokio::time::timeout(Duration::from_secs(60), stream.next())
        .await
        .map_err(|_| "下载超时")?
    {
        let bytes = bytes.map_err(|_| "下载中断")?;
        size += bytes.len() as u64;
        if size > 8 * 1024 * 1024 * 1024 {
            return Err("下载文件超过大小限制".into());
        }
        output.write_all(&bytes).await.map_err(|e| e.to_string())?;
    }
    output.sync_all().await.map_err(|e| e.to_string())?;
    drop(output);
    if size == 0
        || source["size"]
            .as_u64()
            .filter(|s| *s > 0)
            .is_some_and(|expected| expected != size)
    {
        return Err("歌曲下载不完整，请重试".into());
    }
    // Hard-link publication is atomic and fails if another file appeared after the dialog.
    tokio::fs::hard_link(&tmp.0, target)
        .await
        .map_err(|e| format!("无法保存下载（不会覆盖已有文件）: {e}"))?;
    Ok(json!({"path":target,"quality":quality}))
}
