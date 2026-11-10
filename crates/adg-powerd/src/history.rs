//! Bounded power history with exponential smoothing.

use serde::Serialize;
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Sample {
    /// Unix time in seconds.
    pub t: u64,
    /// Raw instantaneous draw in watts, if measurable.
    pub w: Option<f64>,
    /// Smoothed draw in watts.
    pub smooth_w: Option<f64>,
    pub capacity: Option<f64>,
    pub discharging: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WindowStats {
    pub min_w: Option<f64>,
    pub avg_w: Option<f64>,
    pub max_w: Option<f64>,
    pub samples: usize,
}

pub struct History {
    cap: usize,
    alpha: f64,
    samples: VecDeque<Sample>,
    ema: Option<f64>,
    last_discharging: Option<bool>,
}

impl History {
    /// `cap` samples are kept; `alpha` is the EMA weight of the newest sample.
    pub fn new(cap: usize, alpha: f64) -> Self {
        assert!(cap > 0 && alpha > 0.0 && alpha <= 1.0);
        Self { cap, alpha, samples: VecDeque::with_capacity(cap), ema: None, last_discharging: None }
    }

    pub fn push(&mut self, t: u64, w: Option<f64>, capacity: Option<f64>, discharging: bool) -> Sample {
        // A plug/unplug changes the meaning of the reading: restart smoothing.
        if self.last_discharging != Some(discharging) {
            self.ema = None;
        }
        self.last_discharging = Some(discharging);
        if let Some(w) = w.filter(|w| w.is_finite()) {
            self.ema = Some(match self.ema {
                Some(prev) => prev + self.alpha * (w - prev),
                None => w,
            });
        }
        let s = Sample { t, w, smooth_w: self.ema, capacity, discharging };
        if self.samples.len() == self.cap {
            self.samples.pop_front();
        }
        self.samples.push_back(s);
        s
    }

    pub fn smoothed(&self) -> Option<f64> {
        self.ema
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn samples(&self) -> impl Iterator<Item = &Sample> {
        self.samples.iter()
    }

    /// Min/avg/max of raw on-battery readings newer than `since` (unix
    /// seconds). Readings taken on AC are not battery draw and are excluded.
    pub fn stats_since(&self, since: u64) -> WindowStats {
        let vals: Vec<f64> =
            self.samples.iter().filter(|s| s.t >= since && s.discharging).filter_map(|s| s.w).collect();
        if vals.is_empty() {
            return WindowStats::default();
        }
        let sum: f64 = vals.iter().sum();
        WindowStats {
            min_w: vals.iter().copied().reduce(f64::min),
            max_w: vals.iter().copied().reduce(f64::max),
            avg_w: Some(sum / vals.len() as f64),
            samples: vals.len(),
        }
    }
}
