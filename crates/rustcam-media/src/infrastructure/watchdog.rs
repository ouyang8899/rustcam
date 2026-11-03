
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

static HEARTBEAT: AtomicU64 = AtomicU64::new(0);

static LAST_BEAT_MS: AtomicU64 = AtomicU64::new(0);

static START_MS: AtomicU64 = AtomicU64::new(0);

fn origin() -> &'static Instant {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    origin().elapsed().as_millis() as u64
}

pub fn beat() {
    HEARTBEAT.fetch_add(1, Ordering::Relaxed);
    LAST_BEAT_MS.store(now_ms(), Ordering::Relaxed);
}

fn append_line(path: &str, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", line);
        let _ = f.flush();
    }
}

pub fn spawn_watchdog(log_dir: &str) {
    let path = format!("{}/watchdog.log", log_dir);
    let _ = std::fs::create_dir_all(log_dir);

    std::thread::Builder::new()
        .name("rustcam-watchdog".into())
        .spawn(move || {
            let mut last_hb = 0u64;
            let mut stall_secs = 0u64;
            let interval = 3u64;
            loop {
                std::thread::sleep(Duration::from_secs(interval));
                let hb = HEARTBEAT.load(Ordering::Relaxed);
                let last_beat = LAST_BEAT_MS.load(Ordering::Relaxed);
                let now = now_ms();
                let _since_beat = now.saturating_sub(last_beat) / 1000;

                if hb != last_hb {
                    last_hb = hb;
                    stall_secs = 0;
                    continue;
                }

                stall_secs += interval;

                append_line(
                    &path,
                    &format!(
                        "[STALL] heartbeat stopped for {}s (hb={}, threads={:?})",
                        stall_secs,
                        hb,
                        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)
                    ),
                );
            }
        })
        .ok();
}

pub fn spawn_heartbeat() {
    tokio::spawn(async {
        loop {
            beat();
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

pub fn heartbeat() -> u64 {
    HEARTBEAT.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn _unused(_a: Arc<()>) {}
