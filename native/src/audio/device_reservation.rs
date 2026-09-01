#[cfg(target_os = "linux")]
mod linux {
    use crate::audio::backend;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    const PR_SET_PDEATHSIG: i32 = 1;
    const SIGTERM: i32 = 15;

    unsafe extern "C" {
        fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
    }

    /// 对一张声卡的独占预留。持有期间 PipeWire 会让出该声卡的 ALSA 设备，
    /// 我们才能直接打开 hw:；Drop 时结束 pw-reserve 子进程即归还。
    pub(crate) struct DeviceReservation {
        /// 建立这次预留时使用的设备 id，用来判断能否跨会话复用。
        device_id: String,
        /// pw-reserve 子进程。系统上没有 pw-reserve（非 PipeWire 环境）时为
        /// None：此时无人抢占声卡，ALSA 打开 hw: 本身就是独占的。
        child: Option<Child>,
    }

    impl DeviceReservation {
        pub(crate) fn reserve(device_id: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let reservation_name = backend::alsa_reservation_name_for_device_id(device_id)
                .ok_or_else(|| {
                    format!("无法为设备 {} 解析 PipeWire reservation 名称", device_id)
                })?;

            Ok(Self {
                device_id: device_id.to_string(),
                child: spawn_pw_reserve(&reservation_name),
            })
        }

        /// 这次预留是否正好覆盖目标设备（同一端点的不同写法也算命中）。
        pub(crate) fn covers_device(&self, device_id: &str) -> bool {
            backend::output_device_ids_match(&self.device_id, device_id)
        }

        fn release(&mut self) {
            let Some(child) = self.child.as_mut() else {
                return;
            };
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// 起一个 pw-reserve 常驻子进程占住 org.freedesktop.ReserveDevice1.AudioN。
    ///
    /// 预留失败不致命：pw-reserve 不存在（纯 ALSA / PulseAudio 环境）或拿不到
    /// D-Bus 名时，直接打开 hw: 依然是独占的——真占不到设备会在随后的
    /// `wait_for_output_device_available` 里以“设备被占用”明确报错，
    /// 比在这里一律拒绝播放更贴近实际。
    fn spawn_pw_reserve(reservation_name: &str) -> Option<Child> {
        let mut command = Command::new("pw-reserve");
        command
            .args(["-n", reservation_name, "-r", "-a", "ncm-desktop-for-linux"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        unsafe {
            command.pre_exec(|| {
                let result = prctl(PR_SET_PDEATHSIG, SIGTERM as usize, 0, 0, 0);
                if result == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                eprintln!("[audio] 启动 pw-reserve 失败（继续尝试直接独占打开设备）：{error}");
                return None;
            }
        };

        // pw-reserve 拿到 D-Bus 名后会一直驻留；早退说明预留没成功。
        std::thread::sleep(Duration::from_millis(180));
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!(
                    "[audio] pw-reserve 未能预留 {reservation_name}（退出状态 {status}），\
                     继续尝试直接独占打开设备"
                );
                None
            }
            Ok(None) => Some(child),
            Err(error) => {
                eprintln!("[audio] 无法确认 pw-reserve 状态：{error}");
                Some(child)
            }
        }
    }

    impl Drop for DeviceReservation {
        fn drop(&mut self) {
            self.release();
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::DeviceReservation;
