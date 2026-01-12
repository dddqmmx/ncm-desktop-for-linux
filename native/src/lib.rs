mod audio_player;

use napi::{Error, Result};
use napi_derive::napi;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot};
use std::time::Duration;

use crate::audio_player::AudioPlayer;

// --- 全局静态运行时 ---
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("audio-worker")
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

enum PlayerCommand {
    PlayFile(String, Option<u32>),
    PlayUrl(String, Option<u32>),
    Pause,
    Resume,
    Stop,
    Seek(u32),
    WaitFinished(oneshot::Sender<()>),
}

struct SharedState {
    progress_ms: AtomicU32,
    duration_ms: AtomicU32,
    is_playing: AtomicU32, // 0: Stopped, 1: Playing, 2: Paused
}

#[napi]
pub struct PlayerService {
    sender: mpsc::UnboundedSender<PlayerCommand>,
    shared_state: Arc<SharedState>,
}

#[napi]
impl PlayerService {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<PlayerCommand>();
        let shared_state = Arc::new(SharedState {
            progress_ms: AtomicU32::new(0),
            duration_ms: AtomicU32::new(0),
            is_playing: AtomicU32::new(0),
        });

        let state_clone = Arc::clone(&shared_state);
        let rt = get_runtime();

        rt.spawn(async move {
            let mut player = match AudioPlayer::new(None) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("AudioPlayer init failed: {}", e);
                    return;
                }
            };

            // 100ms 更新一次进度，UI 会非常丝滑
            let mut ticker = tokio::time::interval(Duration::from_millis(100));

            loop {
                tokio::select! {
                    // 1. 处理控制指令
                    cmd_opt = rx.recv() => {
                        match cmd_opt {
                            Some(cmd) => match cmd {
                                PlayerCommand::PlayFile(path, start) => {
                                    state_clone.progress_ms.store(0, Ordering::SeqCst);
                                    if player.play_file(&path).await.is_ok() {
                                        // if let Some(s) = start { player.seek(s); }
                                        state_clone.is_playing.store(1, Ordering::SeqCst);
                                    }
                                }
                                PlayerCommand::PlayUrl(url, start) => {
                                    state_clone.progress_ms.store(0, Ordering::SeqCst);
                                    match player.play_url(&url).await {
                                        Ok(_) => {
                                            // if let Some(s) = start { player.seek(s); }
                                            state_clone.is_playing.store(1, Ordering::SeqCst);
                                        }
                                        Err(e) => {
                                            eprintln!("Play URL failed: {}", e);
                                            state_clone.is_playing.store(0, Ordering::SeqCst);
                                            player.stop();
                                        }
                                    }
                                }
                                PlayerCommand::Pause => {
                                    player.pause();
                                    state_clone.is_playing.store(2, Ordering::SeqCst);
                                }
                                PlayerCommand::Resume => {
                                    player.resume();
                                    state_clone.is_playing.store(1, Ordering::SeqCst);
                                }
                                PlayerCommand::Stop => {
                                    player.stop();
                                    state_clone.is_playing.store(0, Ordering::SeqCst);
                                    state_clone.progress_ms.store(0, Ordering::SeqCst);
                                }
                                PlayerCommand::Seek(time) => {
                                    player.seek(time);
                                }
                                PlayerCommand::WaitFinished(done_tx) => {
                                    // 【核心修复】：不要在当前循环内 await player.wait_finished()
                                    // 否则会阻塞整个 loop 导致 ticker 无法运行。
                                    // 既然 wait_finished 本质是等待底层的 Notify，我们单独起一个 task 等待。
                                    let player_state = player.get_state(); // 获取 AudioPlayer 的 SharedState Arc
                                    tokio::spawn(async move {
                                        player_state.finish_notify.notified().await;
                                        let _ = done_tx.send(());
                                    });
                                }
                            },
                            None => break,
                        }
                    }
                    // 2. 状态同步：只要 loop 不被指令 await 阻塞，这里会高频运行
                    _ = ticker.tick() => {
                        let is_playing_val = state_clone.is_playing.load(Ordering::Relaxed);

                        if is_playing_val != 0 {
                            // 更新当前进度
                            let progress = player.progress();
                            state_clone.progress_ms.store(progress.as_millis() as u32, Ordering::Relaxed);

                            // 检查是否播放自然结束
                            if is_playing_val == 1 && player.is_finished() {
                                state_clone.is_playing.store(0, Ordering::SeqCst);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            sender: tx,
            shared_state,
        })
    }

    #[napi]
    pub fn play_file(&self, path: String, start_secs: Option<u32>) -> Result<()> {
        self.sender.send(PlayerCommand::PlayFile(path, start_secs))
            .map_err(|_| Error::from_reason("Background worker died"))
    }

    #[napi]
    pub fn play_url(&self, url: String, start_secs: Option<u32>) -> Result<()> {
        self.sender.send(PlayerCommand::PlayUrl(url, start_secs))
            .map_err(|_| Error::from_reason("Background worker died"))
    }

    #[napi]
    pub fn pause(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Pause);
        Ok(())
    }

    #[napi]
    pub fn resume(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Resume);
        Ok(())
    }

    #[napi]
    pub fn stop(&self) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Stop);
        Ok(())
    }

    #[napi]
    pub fn seek(&self, time_secs: u32) -> Result<()> {
        let _ = self.sender.send(PlayerCommand::Seek(time_secs));
        Ok(())
    }

    // --- Getter 属性：JS 通过 player.progressMs 访问 ---

    #[napi(getter)]
    pub fn progress_ms(&self) -> u32 {
        self.shared_state.progress_ms.load(Ordering::Relaxed)
    }

    #[napi(getter)]
    pub fn is_playing(&self) -> bool {
        self.shared_state.is_playing.load(Ordering::Relaxed) == 1
    }

    #[napi(getter)]
    pub fn is_paused(&self) -> bool {
        self.shared_state.is_playing.load(Ordering::Relaxed) == 2
    }

    // --- 异步 API ---

    #[napi]
    pub async fn wait_finished(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(PlayerCommand::WaitFinished(tx))
            .map_err(|_| Error::from_reason("Worker shutdown"))?;
        rx.await.map_err(|_| Error::from_reason("Playback task interrupted"))
    }
}
