//! Explicit state machine for low-end GPU Liquid Glass safety (`GlassTier`: `Full -> BlurOnly -> Solid`).
//!
//! Picks an initial tier from the GPU adapter class (`Discrete`, `Integrated`, `Software`) plus
//! battery state, then monitors measured glass frame cost with streak and hysteresis guards so
//! weak iGPUs (Intel UHD / Iris Xe, AMD Vega APUs) and software/WARP adapters downgrade smoothly
//! without flapping.

use rune_core::machine::Effects;

use crate::el::GlassTier;

/// Broad GPU adapter capability tier used for initial Liquid Glass policy selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AdapterClass {
    /// Dedicated GPU (e.g. RTX / Radeon RX / Apple Pro/Max discrete-class).
    #[default]
    Discrete,
    /// Integrated GPU / APU (Intel UHD / Iris Xe / HD Graphics, AMD Radeon Vega / APU).
    Integrated,
    /// CPU / software rasterizer (Microsoft Basic Render Driver / WARP / llvmpipe / SwiftShader).
    Software,
}

/// Host power source relevant to iGPU thermal/power budgets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PowerState {
    #[default]
    Ac,
    Battery,
}

/// Classifies a wgpu adapter for Liquid Glass budget defaults.
pub fn classify_adapter(info: &wgpu::AdapterInfo) -> AdapterClass {
    classify_adapter_parts(info.device_type, &info.name)
}

/// Pure helper for classifying an adapter from `DeviceType` and name (directly unit-testable).
pub fn classify_adapter_parts(device_type: wgpu::DeviceType, name: &str) -> AdapterClass {
    let lower = name.to_ascii_lowercase();
    if device_type == wgpu::DeviceType::Cpu
        || lower.contains("basic render driver")
        || lower.contains("warp")
        || lower.contains("llvmpipe")
        || lower.contains("swiftshader")
        || lower.contains("software")
    {
        return AdapterClass::Software;
    }
    if device_type == wgpu::DeviceType::IntegratedGpu
        || lower.contains("intel(r) uhd")
        || lower.contains("intel uhd")
        || lower.contains("iris")
        || lower.contains("hd graphics")
        || lower.contains("vega")
        || lower.contains("radeon(tm) graphics")
    {
        return AdapterClass::Integrated;
    }
    AdapterClass::Discrete
}

/// Consecutive over-budget glass frames required before stepping down one tier.
pub const DOWNGRADE_STREAK: u32 = 6;
/// Consecutive well-under-budget frames required before recovering from `BlurOnly` to `Full`.
pub const UPGRADE_STREAK: u32 = 120;

/// Per-frame GPU cost threshold (ms) above which `Full` steps down to `BlurOnly`.
#[inline]
pub fn full_budget_ms(adapter: AdapterClass) -> f32 {
    match adapter {
        AdapterClass::Discrete => 2.5,
        AdapterClass::Integrated => 1.5,
        AdapterClass::Software => 0.5,
    }
}

/// Per-frame GPU cost threshold (ms) above which `BlurOnly` steps down to `Solid`.
#[inline]
pub fn blur_only_budget_ms(adapter: AdapterClass) -> f32 {
    match adapter {
        AdapterClass::Discrete => 1.8,
        AdapterClass::Integrated => 1.2,
        AdapterClass::Software => 0.5,
    }
}

/// Per-frame GPU cost threshold (ms) below which `BlurOnly` may eventually recover to `Full`.
#[inline]
pub fn recovery_threshold_ms(adapter: AdapterClass) -> f32 {
    match adapter {
        AdapterClass::Discrete => 0.7,
        AdapterClass::Integrated => 0.45,
        AdapterClass::Software => 0.0,
    }
}

/// Explicit state of the Liquid Glass auto-downgrade machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetState {
    Full {
        adapter: AdapterClass,
        power: PowerState,
        over_streak: u32,
    },
    BlurOnly {
        adapter: AdapterClass,
        power: PowerState,
        over_streak: u32,
        under_streak: u32,
        /// Hardware/power floor (e.g. iGPU on battery) — never auto-upgrades above `BlurOnly`.
        locked_by_hardware: bool,
    },
    Solid {
        adapter: AdapterClass,
        power: PowerState,
        locked_by_hardware: bool,
    },
}

impl Default for BudgetState {
    fn default() -> Self {
        Self::Full {
            adapter: AdapterClass::Discrete,
            power: PowerState::Ac,
            over_streak: 0,
        }
    }
}

impl BudgetState {
    pub fn tier(self) -> GlassTier {
        match self {
            Self::Full { .. } => GlassTier::Full,
            Self::BlurOnly { .. } => GlassTier::BlurOnly,
            Self::Solid { .. } => GlassTier::Solid,
        }
    }

    pub fn initial(adapter: AdapterClass, power: PowerState) -> Self {
        match (adapter, power) {
            (AdapterClass::Software, _) => Self::Solid {
                adapter,
                power,
                locked_by_hardware: true,
            },
            (AdapterClass::Integrated, PowerState::Battery) => Self::BlurOnly {
                adapter,
                power,
                over_streak: 0,
                under_streak: 0,
                locked_by_hardware: true,
            },
            _ => Self::Full {
                adapter,
                power,
                over_streak: 0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BudgetEvent {
    Configure {
        adapter: AdapterClass,
        power: PowerState,
    },
    PowerChanged(PowerState),
    /// Measured GPU glass pass cost (ms) for a frame that drew at least one glass surface.
    FrameCostMs(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetEffect {
    TierChanged(GlassTier),
}

/// Pure state-machine transition for Liquid Glass auto-downgrade (`rune::machine` pattern).
pub fn step(state: &mut BudgetState, event: BudgetEvent) -> Effects<BudgetEffect> {
    let before = state.tier();
    match event {
        BudgetEvent::Configure { adapter, power } => {
            *state = BudgetState::initial(adapter, power);
        }
        BudgetEvent::PowerChanged(power) => {
            let adapter = match *state {
                BudgetState::Full { adapter, .. }
                | BudgetState::BlurOnly { adapter, .. }
                | BudgetState::Solid { adapter, .. } => adapter,
            };
            match (*state, adapter, power) {
                (BudgetState::Full { .. }, AdapterClass::Integrated, PowerState::Battery) => {
                    *state = BudgetState::BlurOnly {
                        adapter,
                        power,
                        over_streak: 0,
                        under_streak: 0,
                        locked_by_hardware: true,
                    };
                }
                (
                    BudgetState::BlurOnly {
                        locked_by_hardware: true,
                        ..
                    },
                    AdapterClass::Integrated,
                    PowerState::Ac,
                ) => {
                    *state = BudgetState::Full {
                        adapter,
                        power,
                        over_streak: 0,
                    };
                }
                (BudgetState::Full { over_streak, .. }, _, _) => {
                    *state = BudgetState::Full {
                        adapter,
                        power,
                        over_streak,
                    };
                }
                (
                    BudgetState::BlurOnly {
                        over_streak,
                        under_streak,
                        locked_by_hardware,
                        ..
                    },
                    _,
                    _,
                ) => {
                    *state = BudgetState::BlurOnly {
                        adapter,
                        power,
                        over_streak,
                        under_streak,
                        locked_by_hardware: locked_by_hardware
                            || (adapter == AdapterClass::Integrated
                                && power == PowerState::Battery),
                    };
                }
                (
                    BudgetState::Solid {
                        locked_by_hardware, ..
                    },
                    _,
                    _,
                ) => {
                    *state = BudgetState::Solid {
                        adapter,
                        power,
                        locked_by_hardware,
                    };
                }
            }
        }
        BudgetEvent::FrameCostMs(ms) => {
            if !ms.is_finite() || ms < 0.0 {
                return Effects::none();
            }
            match *state {
                BudgetState::Full {
                    adapter,
                    power,
                    over_streak,
                } => {
                    if ms > full_budget_ms(adapter) {
                        let next = over_streak + 1;
                        if next >= DOWNGRADE_STREAK {
                            *state = BudgetState::BlurOnly {
                                adapter,
                                power,
                                over_streak: 0,
                                under_streak: 0,
                                locked_by_hardware: false,
                            };
                        } else {
                            *state = BudgetState::Full {
                                adapter,
                                power,
                                over_streak: next,
                            };
                        }
                    } else if over_streak > 0 {
                        *state = BudgetState::Full {
                            adapter,
                            power,
                            over_streak: 0,
                        };
                    }
                }
                BudgetState::BlurOnly {
                    adapter,
                    power,
                    over_streak,
                    under_streak,
                    locked_by_hardware,
                } => {
                    if ms > blur_only_budget_ms(adapter) {
                        let next = over_streak + 1;
                        if next >= DOWNGRADE_STREAK {
                            *state = BudgetState::Solid {
                                adapter,
                                power,
                                locked_by_hardware: false,
                            };
                        } else {
                            *state = BudgetState::BlurOnly {
                                adapter,
                                power,
                                over_streak: next,
                                under_streak: 0,
                                locked_by_hardware,
                            };
                        }
                    } else if !locked_by_hardware && ms < recovery_threshold_ms(adapter) {
                        let next = under_streak + 1;
                        if next >= UPGRADE_STREAK {
                            *state = BudgetState::Full {
                                adapter,
                                power,
                                over_streak: 0,
                            };
                        } else {
                            *state = BudgetState::BlurOnly {
                                adapter,
                                power,
                                over_streak: 0,
                                under_streak: next,
                                locked_by_hardware,
                            };
                        }
                    } else if over_streak > 0 || under_streak > 0 {
                        *state = BudgetState::BlurOnly {
                            adapter,
                            power,
                            over_streak: 0,
                            under_streak: 0,
                            locked_by_hardware,
                        };
                    }
                }
                BudgetState::Solid { .. } => {}
            }
        }
    }
    let after = state.tier();
    if after != before {
        Effects::one(BudgetEffect::TierChanged(after))
    } else {
        Effects::none()
    }
}

/// Host-owned auto-downgrade controller wrapping [`BudgetState`] and optional explicit override.
#[derive(Clone, Copy, Debug, Default)]
pub struct GlassBudget {
    state: BudgetState,
    override_tier: Option<GlassTier>,
}

impl GlassBudget {
    #[inline]
    pub fn tier(&self) -> GlassTier {
        self.override_tier.unwrap_or_else(|| self.state.tier())
    }

    pub fn configure(&mut self, adapter: AdapterClass, power: PowerState) -> bool {
        let before = self.tier();
        let _ = step(&mut self.state, BudgetEvent::Configure { adapter, power });
        self.tier() != before
    }

    pub fn set_power(&mut self, power: PowerState) -> bool {
        let before = self.tier();
        let _ = step(&mut self.state, BudgetEvent::PowerChanged(power));
        self.tier() != before
    }

    pub fn record_frame_ms(&mut self, ms: f32) -> bool {
        let before = self.tier();
        let _ = step(&mut self.state, BudgetEvent::FrameCostMs(ms));
        self.tier() != before
    }

    pub fn set_override(&mut self, tier: Option<GlassTier>) -> bool {
        let before = self.tier();
        self.override_tier = tier;
        self.tier() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_warp_igpus_and_discrete_gpus() {
        assert_eq!(
            classify_adapter_parts(wgpu::DeviceType::Cpu, "Microsoft Basic Render Driver"),
            AdapterClass::Software
        );
        assert_eq!(
            classify_adapter_parts(wgpu::DeviceType::IntegratedGpu, "Intel(R) UHD Graphics 770"),
            AdapterClass::Integrated
        );
        assert_eq!(
            classify_adapter_parts(
                wgpu::DeviceType::IntegratedGpu,
                "Intel(R) Iris(R) Xe Graphics"
            ),
            AdapterClass::Integrated
        );
        assert_eq!(
            classify_adapter_parts(wgpu::DeviceType::Other, "AMD Radeon Vega 8 Graphics"),
            AdapterClass::Integrated
        );
        assert_eq!(
            classify_adapter_parts(wgpu::DeviceType::DiscreteGpu, "NVIDIA GeForce RTX 3070"),
            AdapterClass::Discrete
        );
    }

    #[test]
    fn auto_downgrades_by_adapter_battery_and_frame_cost_with_hysteresis() {
        let mut b = GlassBudget::default();
        assert_eq!(b.tier(), GlassTier::Full);

        // WARP / CPU starts directly in Solid.
        assert!(b.configure(AdapterClass::Software, PowerState::Ac));
        assert_eq!(b.tier(), GlassTier::Solid);

        // Integrated GPU on battery starts in BlurOnly, returns to Full on AC.
        b.configure(AdapterClass::Integrated, PowerState::Battery);
        assert_eq!(b.tier(), GlassTier::BlurOnly);
        assert!(b.set_power(PowerState::Ac));
        assert_eq!(b.tier(), GlassTier::Full);

        // Sustained over-budget frames step Full -> BlurOnly -> Solid.
        for _ in 0..(DOWNGRADE_STREAK - 1) {
            assert!(!b.record_frame_ms(2.2));
            assert_eq!(b.tier(), GlassTier::Full);
        }
        assert!(b.record_frame_ms(2.2));
        assert_eq!(b.tier(), GlassTier::BlurOnly);

        // A single cheap frame doesn't flap back; UPGRADE_STREAK recovers when on AC.
        for _ in 0..(UPGRADE_STREAK - 1) {
            assert!(!b.record_frame_ms(0.20));
            assert_eq!(b.tier(), GlassTier::BlurOnly);
        }
        assert!(b.record_frame_ms(0.20));
        assert_eq!(b.tier(), GlassTier::Full);

        // Two consecutive sustained overruns drop all the way to Solid.
        for _ in 0..DOWNGRADE_STREAK {
            b.record_frame_ms(2.5);
        }
        assert_eq!(b.tier(), GlassTier::BlurOnly);
        for _ in 0..DOWNGRADE_STREAK {
            b.record_frame_ms(1.8);
        }
        assert_eq!(b.tier(), GlassTier::Solid);
    }
}
