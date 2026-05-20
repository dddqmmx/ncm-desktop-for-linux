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

    pub(crate) struct DeviceReservation {
        child: Child,
    }

    impl DeviceReservation {
        pub(crate) fn reserve(device_id: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let reservation_name = backend::alsa_reservation_name_for_device_id(device_id)
                .ok_or_else(|| {
                    format!("无法为设备 {} 解析 PipeWire reservation 名称", device_id)
                })?;

            let mut command = Command::new("pw-reserve");
            command
                .args(["-n", &reservation_name, "-r", "-a", "ncm-desktop-for-linux"])
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

            let mut child = command
                .spawn()
                .map_err(|error| format!("启动 pw-reserve 失败：{error}"))?;

            std::thread::sleep(Duration::from_millis(180));
            if let Some(status) = child.try_wait()? {
                return Err(format!(
                    "pw-reserve 未能预留 {}：退出状态 {}",
                    reservation_name, status
                )
                .into());
            }

            Ok(Self { child })
        }

        fn release(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
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
