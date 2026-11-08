//! 输出端点的身份：id 写法、等价判定、Linux 上的 /proc/asound 归一。
//!
//! 这一层只做**纯函数**（只有 `device_id` / `device_display_name` 需要 cpal 设备
//! 句柄），因此全部可以在没有声卡的机器上测。它也承载了本项目最反直觉的一条
//! 约定：同一物理端点在 ALSA 上有多种 id 写法，任何字符串等值比较都会失配。
//!
//! 相关：`cpal::Device::id().1` 才是可重新打开设备的标识，
//! `description().name()` 是给人看的显示名。

use cpal::traits::DeviceTrait;
use std::path::Path;
use std::time::Duration;

use super::OutputDeviceInfo;

/// 设备 id：后端用来重新打开设备的原始标识（ALSA 上就是 PCM 名，
/// 形如 `hw:CARD=1,DEV=0` / `default` / `pipewire`）。
///
/// 注意不能用 `description().name()`：cpal 0.17 起那是给人看的显示名
/// （USB DAC 上是 "FreeDSP Mini, USB Audio"），拿它当 id 会让
/// `find_output_device` 的等值匹配、`is_current` 判定、以及独占模式的
/// “是否真实硬件端点”校验全部失效。`DeviceId` 的第二个字段才是后端标识，
/// 与旧版 `DeviceTrait::name()`（现已废弃）取到的是同一个串。
pub(crate) fn device_id(device: &cpal::Device) -> String {
    device.id().map(|id| id.1).unwrap_or_default()
}

/// 设备显示名：cpal 已把 ALSA hint 的描述首行（以及物理设备的
/// “声卡名, PCM 名”）整理进 description，无需自己再解析一遍 hint。
pub(crate) fn device_display_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| device_id(device))
}

/// 两个输出设备 id 是否指向同一个可打开端点。
///
/// Linux 上同一物理端点存在两种写法：`hw:CARD=1,DEV=0`（数字卡号，来自
/// /proc/asound 与 cpal 的物理探测）和 `hw:CARD=Mini,DEV=0`（字符卡名，来自
/// ALSA hint），字符串不等但指向同一个端点。`plughw:` 与 `hw:` 端点相同却一个
/// 带软件转换、一个直通，BitPerfect 下不可互换，所以端点键之外还要比对前缀。
pub(crate) fn output_device_ids_match(left: &str, right: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        output_device_ids_match_at(left, right, std::path::Path::new("/proc/asound"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        left == right
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn output_device_ids_match_at(left: &str, right: &str, asound_root: &Path) -> bool {
    if left == right {
        return true;
    }

    match (
        linux_hw_endpoint_key_at(left, asound_root),
        linux_hw_endpoint_key_at(right, asound_root),
    ) {
        (Some(left_key), Some(right_key)) => {
            left_key == right_key
                && is_linux_plughw_output_id(left) == is_linux_plughw_output_id(right)
        }
        _ => false,
    }
}

/// 等待输出设备可被打开。独占播放要先让 PipeWire 交还声卡，释放是异步的：
/// 预留刚拿到手时 `hw:` 往往还是 EBUSY，直接开流就会以“设备不可用”失败。
/// `supported_output_configs()` 内部就是一次 PCM open，正好当作可用性探针。
pub(crate) fn wait_for_output_device_available(
    device: &cpal::Device,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = std::time::Instant::now() + timeout;

    loop {
        match device.supported_output_configs() {
            Ok(_) => return Ok(()),
            Err(cpal::SupportedStreamConfigsError::DeviceNotAvailable) => {
                if std::time::Instant::now() >= deadline {
                    return Err(format!(
                        "无法启用独占输出：设备 {} 正被其他程序占用或已断开，\
                         请关闭占用该设备的程序后重试",
                        device_id(device)
                    )
                    .into());
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// 把 hw:/plughw: 设备 id 解析为 (card_index, device_index) 端点键。
/// UI 侧的 id 可能来自 /proc/asound（数字 CARD=1），cpal/ALSA hint 的 id
/// 则用字符卡名（CARD=UsbDac）；两种写法字符串不相等但指向同一物理端点，
/// 必须归一到数字索引后再比较。
#[cfg(target_os = "linux")]
pub(crate) fn linux_hw_endpoint_key(id: &str) -> Option<(u32, u32)> {
    linux_hw_endpoint_key_at(id, std::path::Path::new("/proc/asound"))
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_hw_endpoint_key_at(id: &str, asound_root: &Path) -> Option<(u32, u32)> {
    if !is_linux_real_hardware_output_id(id) {
        return None;
    }
    let card_index = alsa_card_index_from_device_id_at(id, asound_root)?;
    let device_index = extract_alsa_device_token(id).unwrap_or(0);
    Some((card_index, device_index))
}

#[cfg(target_os = "linux")]
pub(crate) fn is_linux_plughw_output_id(id: &str) -> bool {
    id.trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim()
        .to_ascii_lowercase()
        .starts_with("plughw:")
}

pub(crate) fn should_list_linux_output_device(
    id: &str,
    _default_id: Option<&str>,
    _current_id: Option<&str>,
) -> bool {
    let normalized = id.trim().to_ascii_lowercase();
    if is_linux_real_hardware_output_id(&normalized) {
        return true;
    }

    false
}

#[cfg(target_os = "linux")]
pub(crate) fn append_linux_proc_asound_output_devices(
    devices: &mut Vec<OutputDeviceInfo>,
    default_id: Option<&str>,
    current_id: Option<&str>,
) {
    append_linux_proc_asound_output_devices_at(
        devices,
        default_id,
        current_id,
        std::path::Path::new("/proc/asound"),
    );
}

#[cfg(target_os = "linux")]
pub(crate) fn append_linux_proc_asound_output_devices_at(
    devices: &mut Vec<OutputDeviceInfo>,
    default_id: Option<&str>,
    current_id: Option<&str>,
    asound_root: &Path,
) {
    let cards = parse_proc_asound_cards(&asound_root.join("cards"));
    let Ok(pcm) = std::fs::read_to_string(asound_root.join("pcm")) else {
        return;
    };

    for line in pcm.lines() {
        let Some((card_index, device_index, pcm_name)) = parse_proc_asound_playback_pcm_line(line)
        else {
            continue;
        };
        let Some(card) = cards.iter().find(|card| card.index == card_index) else {
            continue;
        };

        // 规范写法：字符卡名跨拔插稳定（数字卡号会随 USB 枚举顺序漂移），
        // 由它充当条目 id，持久化身份才不会插回一次就永久失效。
        let numeric_id = format!("hw:CARD={},DEV={}", card.index, device_index);
        let id = canonical_linux_output_device_id_at(&numeric_id, asound_root);
        let name = format!("{} - {}", card.display_name(), pcm_name);
        // 归一匹配必须用同一份 asound_root（测试传的是临时目录；不带 _at 的
        // 变体会读真实 /proc/asound，规范化 id 在测试环境里永远归一不上）。
        let is_default =
            default_id.is_some_and(|value| output_device_ids_match_at(value, &id, asound_root));
        let is_current =
            current_id.is_some_and(|value| output_device_ids_match_at(value, &id, asound_root));

        if let Some(device) = devices.iter_mut().find(|device| device.id == id) {
            if device.name == device.id || device.name.trim().is_empty() {
                device.name = name;
            }
            device.is_default |= is_default;
            device.is_current |= is_current;
            continue;
        }

        devices.push(OutputDeviceInfo {
            is_default,
            is_current,
            aliases: linux_output_device_id_aliases_at(&id, asound_root),
            id,
            name,
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn collapse_linux_duplicate_output_devices(devices: &mut Vec<OutputDeviceInfo>) {
    collapse_linux_duplicate_output_devices_at(devices, std::path::Path::new("/proc/asound"));
}

/// 同一物理端点会以多种 id 出现（cpal hint 的字符卡名 hw:/plughw:、
/// /proc/asound 与 cpal 物理探测的数字卡号），按 (card, device) 端点键合并成
/// 一条：条目 id 规范化为字符卡名写法（跨拔插稳定，数字卡号会随 USB 枚举
/// 顺序漂移），别名为全写法集合——前端的持久化身份与激活匹配改走别名表，
/// 不再依赖“合并后条目的 id 恰好等于前端持久化写法”的巧合；显示名取非
/// 占位的一侧，hw:/plughw: 属不同输出端点仍互不合并。
#[cfg(target_os = "linux")]
pub(crate) fn collapse_linux_duplicate_output_devices_at(
    devices: &mut Vec<OutputDeviceInfo>,
    asound_root: &Path,
) {
    use std::collections::HashMap;

    let mut kept_by_key: HashMap<(u32, u32), usize> = HashMap::new();
    let mut result: Vec<OutputDeviceInfo> = Vec::new();

    for device in devices.drain(..) {
        let Some(key) = linux_hw_endpoint_key_at(&device.id, asound_root) else {
            let mut device = device;
            if device.aliases.is_empty() {
                device.aliases = linux_output_device_id_aliases_at(&device.id, asound_root);
            }
            result.push(device);
            continue;
        };

        let canonical_id = canonical_linux_output_device_id_at(&device.id, asound_root);

        let Some(&index) = kept_by_key.get(&key) else {
            let aliases = linux_output_device_id_aliases_at(&canonical_id, asound_root);
            let mut device = device;
            device.id = canonical_id;
            if device.aliases.is_empty() {
                device.aliases = aliases;
            } else {
                merge_aliases(&mut device.aliases, &aliases);
            }
            kept_by_key.insert(key, result.len());
            result.push(device);
            continue;
        };

        let incoming_aliases = linux_output_device_id_aliases_at(&canonical_id, asound_root);
        let existing = &mut result[index];
        let existing_placeholder = existing.name == existing.id || existing.name.trim().is_empty();
        let incoming_placeholder = device.name == device.id || device.name.trim().is_empty();

        // 数字卡号条目的名字多半来自 /proc 追加（已填真实显示名），优先于
        // cpal 物理探测的占位名。
        if existing_placeholder && !incoming_placeholder {
            existing.name = device.name.clone();
        }

        // 合并后的 id 固定用规范写法；current 标记与别名为并集。
        existing.id = canonical_linux_output_device_id_at(&existing.id, asound_root);
        let existing_aliases = linux_output_device_id_aliases_at(&existing.id, asound_root);
        merge_aliases(&mut existing.aliases, &existing_aliases);
        merge_aliases(&mut existing.aliases, &incoming_aliases);
        existing.is_default |= device.is_default;
        existing.is_current |= device.is_current;
    }

    *devices = result;
}

fn merge_aliases(target: &mut Vec<String>, extra: &[String]) {
    for alias in extra {
        if !target.contains(alias) {
            target.push(alias.clone());
        }
    }
}

pub(crate) fn is_linux_real_hardware_output_id(id: &str) -> bool {
    let normalized = id.trim().to_ascii_lowercase();
    normalized.starts_with("hw:")
        || normalized.starts_with("plughw:")
        || normalized.starts_with("alsa:hw:")
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcAsoundCard {
    index: u32,
    id: String,
    name: String,
    long_name: String,
}

#[cfg(target_os = "linux")]
impl ProcAsoundCard {
    fn display_name(&self) -> String {
        let long_name = self.long_name.trim();
        if !long_name.is_empty() {
            return long_name
                .split_once(" at ")
                .map(|(name, _)| name.trim())
                .unwrap_or(long_name)
                .to_string();
        }

        self.name.clone()
    }
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_cards(path: &std::path::Path) -> Vec<ProcAsoundCard> {
    let Ok(cards) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut result = Vec::new();
    let mut lines = cards.lines();
    while let Some(header) = lines.next() {
        let Some(card) = parse_proc_asound_card_header(header) else {
            continue;
        };
        let long_name = lines.next().unwrap_or_default().trim().to_string();
        result.push(ProcAsoundCard { long_name, ..card });
    }
    result
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_card_header(line: &str) -> Option<ProcAsoundCard> {
    let (index_text, rest) = line.trim_start().split_once(' ')?;
    let index = index_text.parse::<u32>().ok()?;
    let bracket_start = rest.find('[')?;
    let bracket_end = rest[bracket_start + 1..].find(']')? + bracket_start + 1;
    let id = rest[bracket_start + 1..bracket_end].trim().to_string();
    let (_, name_part) = rest[bracket_end + 1..].split_once(':')?;
    let name = name_part
        .split_once('-')
        .map(|(_, value)| value.trim())
        .unwrap_or_else(|| name_part.trim())
        .to_string();

    if id.is_empty() {
        return None;
    }

    Some(ProcAsoundCard {
        index,
        id,
        name,
        long_name: String::new(),
    })
}

#[cfg(target_os = "linux")]
fn parse_proc_asound_playback_pcm_line(line: &str) -> Option<(u32, u32, String)> {
    if !line.contains("playback") {
        return None;
    }

    let (address, rest) = line.split_once(':')?;
    let (card_text, device_text) = address.trim().split_once('-')?;
    let card_index = card_text.parse::<u32>().ok()?;
    let device_index = device_text.parse::<u32>().ok()?;
    let pcm_name = rest
        .split(':')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("PCM")
        .to_string();

    Some((card_index, device_index, pcm_name))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_reservation_name_for_device_id(id: &str) -> Option<String> {
    alsa_card_index_from_device_id(id).map(|card_index| format!("Audio{card_index}"))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_card_index_from_device_id(id: &str) -> Option<u32> {
    alsa_card_index_from_device_id_at(id, std::path::Path::new("/proc/asound"))
}

#[cfg(target_os = "linux")]
pub(crate) fn alsa_card_index_from_device_id_at(id: &str, asound_root: &Path) -> Option<u32> {
    let card_token = extract_alsa_card_token(id)?;

    if let Ok(card_index) = card_token.parse::<u32>() {
        return Some(card_index);
    }

    let entries = std::fs::read_dir(asound_root).ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(card_index) = file_name
            .strip_prefix("card")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let card_id_path = entry.path().join("id");
        let Ok(card_id) = std::fs::read_to_string(card_id_path) else {
            continue;
        };

        if card_id.trim() == card_token {
            return Some(card_index);
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn alsa_card_id_from_index_at(card_index: u32, asound_root: &Path) -> Option<String> {
    let card_id_path = asound_root.join(format!("card{card_index}")).join("id");
    let card_id = std::fs::read_to_string(card_id_path).ok()?;
    let trimmed = card_id.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// 把设备 id 规范化为跨拔插稳定的写法：hw:/plughw: 端点的数字卡号换成字符
/// 卡名（`hw:CARD=1,DEV=0` → `hw:CARD=UsbDac,DEV=0`），其余原样返回。
///
/// 数字卡号在拔插后可能漂移（USB 枚举顺序变化），拿它做持久化身份，插回一次
/// 就永久失效；字符卡名由 ALSA 按卡身份解析，插回后哪怕卡号变了仍能打开。
/// 无法解析字符卡名（读不到 /proc/asound）时返回原 id——稳定写法不可得时，
/// 至少不能让枚举凭空丢设备。
#[cfg(target_os = "linux")]
pub(crate) fn canonical_linux_output_device_id_at(id: &str, asound_root: &Path) -> String {
    if !is_linux_real_hardware_output_id(id) {
        return id.to_string();
    }

    let Some(card_index) = alsa_card_index_from_device_id_at(id, asound_root) else {
        return id.to_string();
    };
    // 已是字符写法的不再翻回数字：目标是稳定，不是统一形状。
    let card_token = extract_alsa_card_token(id);
    if card_token
        .as_deref()
        .and_then(|token| token.parse::<u32>().ok())
        .is_none()
    {
        return id.to_string();
    }

    let Some(card_id) = alsa_card_id_from_index_at(card_index, asound_root) else {
        return id.to_string();
    };

    id.replace(&format!("CARD={card_index}"), &format!("CARD={card_id}"))
}

/// 同一物理端点在各命名空间的全部写法（去重）。前端拿它与持久化的 id 做匹配，
/// 弥补渲染层做不了 /proc/asound 归一比较的缺口：hw:CARD=1,DEV=0（数字卡号，
/// cpal 物理探测 / /proc/asound）、hw:CARD=UsbDac,DEV=0（字符卡名，ALSA hint）
/// 以及 plughw: 变体。注意 hw:/plughw: 是不同输出端点（一个直通一个带软件
/// 转换），互不进对方的别名表。
#[cfg(target_os = "linux")]
pub(crate) fn linux_output_device_id_aliases_at(id: &str, asound_root: &Path) -> Vec<String> {
    let mut aliases = vec![id.to_string()];
    if !is_linux_real_hardware_output_id(id) {
        return aliases;
    }

    let Some(card_index) = alsa_card_index_from_device_id_at(id, asound_root) else {
        return aliases;
    };
    let card_tokens: [String; 2] = [
        card_index.to_string(),
        alsa_card_id_from_index_at(card_index, asound_root)
            .unwrap_or_else(|| card_index.to_string()),
    ];

    let prefix = if is_linux_plughw_output_id(id) {
        "plughw"
    } else {
        "hw"
    };
    let device_index = extract_alsa_device_token(id).unwrap_or(0);

    for token in card_tokens {
        let alias = format!("{prefix}:CARD={token},DEV={device_index}");
        if !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }

    aliases
}

#[cfg(target_os = "linux")]
fn extract_alsa_card_token(id: &str) -> Option<String> {
    let id = id
        .trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim();
    let id = id
        .strip_prefix("hw:")
        .or_else(|| id.strip_prefix("plughw:"))?
        .trim();

    if let Some(rest) = id.strip_prefix("CARD=") {
        return Some(rest.split(',').next()?.trim().to_string()).filter(|token| !token.is_empty());
    }

    Some(id.split(',').next()?.trim().to_string()).filter(|token| !token.is_empty())
}

#[cfg(target_os = "linux")]
fn extract_alsa_device_token(id: &str) -> Option<u32> {
    let id = id
        .trim()
        .strip_prefix("alsa:")
        .unwrap_or_else(|| id.trim())
        .trim();
    let id = id
        .strip_prefix("hw:")
        .or_else(|| id.strip_prefix("plughw:"))?
        .trim();

    if let Some((_, rest)) = id.split_once("DEV=") {
        return rest
            .split(',')
            .next()
            .and_then(|value| value.trim().parse::<u32>().ok());
    }

    id.split(',')
        .nth(1)
        .and_then(|value| value.trim().parse::<u32>().ok())
}

pub(crate) fn disambiguate_output_device_names(devices: &mut [OutputDeviceInfo]) {
    use std::collections::HashMap;
    let mut name_counts = HashMap::new();
    for d in devices.iter() {
        *name_counts.entry(d.name.clone()).or_insert(0) += 1;
    }

    for d in devices.iter_mut() {
        if *name_counts.get(&d.name).unwrap() > 1 {
            d.name = format!("{} [{}]", d.name, d.id);
        }
    }
}
