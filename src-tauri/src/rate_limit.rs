use std::time::{Duration, Instant};

pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(rate_bytes_per_sec: u64) -> Self {
        let rate = rate_bytes_per_sec as f64;
        Self {
            capacity: rate, // 1 second burst
            tokens: rate,
            refill_rate: rate,
            last_refill: Instant::now(),
        }
    }

    pub fn acquire(&mut self, amount: u64) -> bool {
        self.refill();
        if self.tokens >= amount as f64 {
            self.tokens -= amount as f64;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;
        
        if new_tokens > 0.0 {
            self.tokens = (self.tokens + new_tokens).min(self.capacity);
            self.last_refill = now;
        }
    }
}

pub struct SpeedMonitor {
    window_size: Duration,
    samples: Vec<(Instant, u64)>,
}

impl SpeedMonitor {
    pub fn new(window_secs: u64) -> Self {
        Self {
            window_size: Duration::from_secs(window_secs),
            samples: Vec::new(),
        }
    }

    pub fn add_sample(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push((now, bytes));
        self.cleanup(now);
    }

    fn cleanup(&mut self, now: Instant) {
        while let Some((time, _)) = self.samples.first() {
            if now.duration_since(*time) > self.window_size {
                self.samples.remove(0);
            } else {
                break;
            }
        }
    }

    pub fn current_speed(&self) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        
        let total_bytes: u64 = self.samples.iter().map(|(_, b)| b).sum();
        
        // If we have less than window size of data, we should still calculate based on actual elapsed time
        // But for stability, let's just use the window size as denominator if we have enough samples,
        // or the actual time span if it's short.
        let first = self.samples.first().unwrap().0;
        let last = self.samples.last().unwrap().0;
        let elapsed = last.duration_since(first).as_secs_f64();
        
        if elapsed < 1.0 {
            // Avoid division by zero or huge spikes for very short intervals
            return total_bytes; 
        }

        (total_bytes as f64 / elapsed.max(1.0)) as u64
    }
}
