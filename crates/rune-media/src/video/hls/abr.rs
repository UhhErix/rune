//! Adaptive bitrate (ABR) variant selection: start low, step up on sustained throughput and
//! healthy buffer, step down immediately on low throughput or low buffer, and cap resolution by
//! the element's size on screen.

#![forbid(unsafe_code)]

use super::playlist::Variant;
use std::time::Duration;

/// Minimal variant metadata needed by the ABR decision function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbrVariant {
    /// Original index in the master playlist's variant slice.
    pub index: usize,
    pub bandwidth: u64,
    pub resolution: Option<(u32, u32)>,
    pub codecs: Option<String>,
}

impl AbrVariant {
    pub fn from_playlist(variants: &[Variant]) -> Vec<Self> {
        variants
            .iter()
            .enumerate()
            .map(|(index, v)| Self {
                index,
                bandwidth: v.average_bandwidth.unwrap_or(v.bandwidth).max(1),
                resolution: v.resolution,
                codecs: v.codecs.clone(),
            })
            .collect()
    }
}

/// Tuning knobs for adaptive bitrate switching.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbrConfig {
    /// Fraction of measured throughput required to cover the next higher variant's bandwidth.
    pub safety_factor_up: f64,
    /// Fraction of measured throughput below which we step down from the current variant.
    pub safety_factor_down: f64,
    /// Minimum buffered duration (seconds) required before stepping up.
    pub min_buffer_for_step_up_secs: f64,
    /// Buffer duration (seconds) below which we step down immediately.
    pub low_buffer_step_down_secs: f64,
    /// Exponentially-weighted moving average smoothing factor for throughput samples.
    pub ewma_alpha: f64,
    /// Number of consecutive sustainable samples required before stepping up one rung.
    pub step_up_confirmations: u32,
}

impl Default for AbrConfig {
    fn default() -> Self {
        Self {
            safety_factor_up: 0.75,
            safety_factor_down: 0.85,
            min_buffer_for_step_up_secs: 4.0,
            low_buffer_step_down_secs: 2.0,
            ewma_alpha: 0.35,
            step_up_confirmations: 2,
        }
    }
}

/// Inputs to a single pure ABR decision step.
#[derive(Clone, Debug)]
pub struct AbrInput<'a> {
    pub variants: &'a [AbrVariant],
    pub current_index: usize,
    pub throughput_bps: f64,
    pub buffer_secs: f64,
    /// Physical pixel size `(width, height)` of the video element on screen, if known.
    pub display_size: Option<(u32, u32)>,
    pub consecutive_high: u32,
}

/// Output of a single ABR decision step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbrDecision {
    pub variant_index: usize,
    pub next_consecutive_high: u32,
}

/// Returns true when `codecs` appears to carry a video stream compatible with Media Foundation /
/// AVFoundation (H.264 `avc1`/`avc3`, HEVC `hvc1`/`hev1`, VP9, AV1) or when codecs aren't listed.
fn is_playable_video(variant: &AbrVariant) -> bool {
    let Some(codecs) = variant.codecs.as_deref() else {
        return true;
    };
    let lower = codecs.to_ascii_lowercase();
    let has_video = lower.contains("avc1")
        || lower.contains("avc3")
        || lower.contains("hvc1")
        || lower.contains("hev1")
        || lower.contains("vp09")
        || lower.contains("av01");
    if has_video {
        return true;
    }
    // Reject explicitly audio-only variants when mixed with video variants.
    !(lower.contains("mp4a")
        || lower.contains("ac-3")
        || lower.contains("ec-3")
        || lower.contains("opus"))
}

/// Filters variants to those playable and capped by `display_size` (sorted by bandwidth ascending).
/// If every variant exceeds `display_size`, keeps the smallest-resolution (tie-break lowest
/// bandwidth) variant so playback always starts.
fn eligible_ladder(variants: &[AbrVariant], display_size: Option<(u32, u32)>) -> Vec<&AbrVariant> {
    if variants.is_empty() {
        return Vec::new();
    }
    let playable: Vec<&AbrVariant> = {
        let v: Vec<&AbrVariant> = variants.iter().filter(|v| is_playable_video(v)).collect();
        if v.is_empty() {
            variants.iter().collect()
        } else {
            v
        }
    };

    let mut capped: Vec<&AbrVariant> = match display_size.filter(|&(w, h)| w > 0 && h > 0) {
        Some((dw, dh)) => {
            let within: Vec<&AbrVariant> = playable
                .iter()
                .copied()
                .filter(|v| match v.resolution {
                    Some((vw, vh)) => vw <= dw && vh <= dh,
                    None => true,
                })
                .collect();
            if within.is_empty() {
                // Element is smaller than every rung: keep the smallest rung so playback still works.
                let smallest = playable.iter().copied().min_by_key(|v| {
                    let area = v
                        .resolution
                        .map_or(u64::MAX, |(w, h)| u64::from(w) * u64::from(h));
                    (area, v.bandwidth)
                });
                smallest.into_iter().collect()
            } else {
                within
            }
        }
        None => playable,
    };

    capped.sort_by_key(|v| (v.bandwidth, v.index));
    capped
}

/// Chooses the initial variant: starts at the lowest-bandwidth eligible rung for fast startup.
pub fn select_initial_variant(
    variants: &[AbrVariant],
    display_size: Option<(u32, u32)>,
) -> Option<usize> {
    eligible_ladder(variants, display_size)
        .first()
        .map(|v| v.index)
}

/// Pure adaptive bitrate decision function.
pub fn decide_variant(input: &AbrInput<'_>, config: &AbrConfig) -> AbrDecision {
    let ladder = eligible_ladder(input.variants, input.display_size);
    if ladder.is_empty() {
        return AbrDecision {
            variant_index: input.current_index,
            next_consecutive_high: 0,
        };
    }

    let throughput = if input.throughput_bps.is_finite() && input.throughput_bps > 0.0 {
        input.throughput_bps
    } else {
        0.0
    };
    let buffer = if input.buffer_secs.is_finite() {
        input.buffer_secs.max(0.0)
    } else {
        0.0
    };

    // If the active variant now exceeds the element's size cap, drop immediately to the highest
    // eligible rung sustainable by current throughput (or the lowest eligible rung).
    let Some(cur_rank) = ladder.iter().position(|v| v.index == input.current_index) else {
        let target = highest_fitting(&ladder, throughput * config.safety_factor_down).unwrap_or(0);
        return AbrDecision {
            variant_index: ladder[target].index,
            next_consecutive_high: 0,
        };
    };

    let current = ladder[cur_rank];

    // Step down immediately when buffer is critically low.
    if buffer < config.low_buffer_step_down_secs && cur_rank > 0 {
        let by_bw = highest_fitting(&ladder, throughput * config.safety_factor_down).unwrap_or(0);
        let target = by_bw.min(cur_rank - 1);
        return AbrDecision {
            variant_index: ladder[target].index,
            next_consecutive_high: 0,
        };
    }

    // Step down immediately when measured throughput cannot sustain the active variant.
    if throughput > 0.0
        && throughput * config.safety_factor_down < current.bandwidth as f64
        && cur_rank > 0
    {
        let target = highest_fitting(&ladder, throughput * config.safety_factor_down)
            .unwrap_or(0)
            .min(cur_rank - 1);
        return AbrDecision {
            variant_index: ladder[target].index,
            next_consecutive_high: 0,
        };
    }

    // Step up smoothly (one rung at a time) when buffer is healthy and throughput has headroom.
    if buffer >= config.min_buffer_for_step_up_secs && cur_rank + 1 < ladder.len() {
        let next = ladder[cur_rank + 1];
        if throughput * config.safety_factor_up >= next.bandwidth as f64 {
            let streak = input.consecutive_high.saturating_add(1);
            if streak >= config.step_up_confirmations.max(1) {
                return AbrDecision {
                    variant_index: next.index,
                    next_consecutive_high: 0,
                };
            }
            return AbrDecision {
                variant_index: current.index,
                next_consecutive_high: streak,
            };
        }
    }

    AbrDecision {
        variant_index: current.index,
        next_consecutive_high: 0,
    }
}

fn highest_fitting(ladder: &[&AbrVariant], budget_bps: f64) -> Option<usize> {
    if budget_bps <= 0.0 {
        return None;
    }
    ladder
        .iter()
        .rposition(|v| (v.bandwidth as f64) <= budget_bps)
}

/// Stateful ABR controller tracking EWMA throughput and step-up hysteresis.
#[derive(Clone, Debug)]
pub struct AbrController {
    config: AbrConfig,
    variants: Vec<AbrVariant>,
    current_index: usize,
    throughput_bps: f64,
    consecutive_high: u32,
}

impl AbrController {
    pub fn new(
        variants: Vec<AbrVariant>,
        display_size: Option<(u32, u32)>,
        config: AbrConfig,
    ) -> Self {
        let current_index = select_initial_variant(&variants, display_size).unwrap_or(0);
        Self {
            config,
            variants,
            current_index,
            throughput_bps: 0.0,
            consecutive_high: 0,
        }
    }

    pub fn current_index(&self) -> usize {
        self.current_index
    }

    pub fn throughput_bps(&self) -> f64 {
        self.throughput_bps
    }

    /// Incorporates one completed segment transfer into the throughput EWMA.
    pub fn record_sample(&mut self, bytes: usize, elapsed: Duration) {
        if bytes == 0 {
            return;
        }
        let secs = elapsed.as_secs_f64().max(0.001);
        let sample_bps = (bytes as f64 * 8.0) / secs;
        if !sample_bps.is_finite() || sample_bps <= 0.0 {
            return;
        }
        if self.throughput_bps <= 0.0 {
            self.throughput_bps = sample_bps;
        } else {
            let a = self.config.ewma_alpha.clamp(0.05, 1.0);
            self.throughput_bps = a * sample_bps + (1.0 - a) * self.throughput_bps;
        }
    }

    /// Evaluates whether to stay on, step up, or step down from the current variant.
    pub fn select(&mut self, buffer_secs: f64, display_size: Option<(u32, u32)>) -> usize {
        let decision = decide_variant(
            &AbrInput {
                variants: &self.variants,
                current_index: self.current_index,
                throughput_bps: self.throughput_bps,
                buffer_secs,
                display_size,
                consecutive_high: self.consecutive_high,
            },
            &self.config,
        );
        self.current_index = decision.variant_index;
        self.consecutive_high = decision.next_consecutive_high;
        self.current_index
    }
}
