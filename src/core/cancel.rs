use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn sleep(&self, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            if self.is_cancelled() {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(remaining.min(Duration::from_millis(50)));
        }
        !self.is_cancelled()
    }

    pub fn bounded(&self, duration: Duration) -> CancelToken {
        let combined = CancelToken::new();
        let parent = self.clone();
        let trigger = combined.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + duration;
            while !parent.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            trigger.cancel();
        });
        combined
    }
}
