use super::{MemoryPressure, MemoryReserveThresholds};

const MIB: u64 = 1024 * 1024;
const SHRINKING_TARGET_PERCENT: u64 = 20;
const SHRINKING_MIN_BYTES: u64 = 1536 * MIB;
const SHRINKING_MAX_BYTES: u64 = 4096 * MIB;
const EMERGENCY_TARGET_PERCENT: u64 = 10;
const EMERGENCY_MIN_BYTES: u64 = 768 * MIB;
const EMERGENCY_MAX_BYTES: u64 = 2048 * MIB;
const HARD_EMERGENCY_BYTES: u64 = 512 * MIB;

const ENTER_SAMPLES: u8 = 2;
const EXIT_SAMPLES: u8 = 3;
const EXIT_MARGIN_BYTES: u64 = 512 * MIB;

pub fn memory_reserve_thresholds(total: u64) -> MemoryReserveThresholds {
    if total == 0 {
        return MemoryReserveThresholds {
            shrinking_bytes: 0,
            emergency_bytes: 0,
        };
    }
    MemoryReserveThresholds {
        shrinking_bytes: reserve(
            total,
            SHRINKING_TARGET_PERCENT,
            SHRINKING_MIN_BYTES,
            SHRINKING_MAX_BYTES,
        ),
        emergency_bytes: reserve(
            total,
            EMERGENCY_TARGET_PERCENT,
            EMERGENCY_MIN_BYTES,
            EMERGENCY_MAX_BYTES,
        ),
    }
}

pub fn is_hard_emergency(total: u64, available: u64) -> bool {
    total > 0 && available < hard_emergency_reserve(total)
}

pub fn classify_pressure(total: u64, available: u64) -> MemoryPressure {
    if total == 0 {
        return MemoryPressure::Unknown;
    }
    let thresholds = memory_reserve_thresholds(total);
    if available < thresholds.emergency_bytes {
        return MemoryPressure::Emergency;
    }
    if available < thresholds.shrinking_bytes {
        return MemoryPressure::Shrinking;
    }
    MemoryPressure::Comfortable
}

fn reserve(total: u64, percent: u64, minimum: u64, maximum: u64) -> u64 {
    (total.saturating_mul(percent) / 100)
        .max(minimum)
        .min(maximum)
        .min(total)
}

fn hard_emergency_reserve(total: u64) -> u64 {
    HARD_EMERGENCY_BYTES.min(total)
}

pub struct MemoryPressureTracker {
    state: MemoryPressure,
    pending: Option<MemoryPressure>,
    samples: u8,
}

impl Default for MemoryPressureTracker {
    fn default() -> Self {
        Self {
            state: MemoryPressure::Comfortable,
            pending: None,
            samples: 0,
        }
    }
}

impl MemoryPressureTracker {
    pub fn current(&self) -> MemoryPressure {
        self.state
    }

    pub fn observe(&mut self, total: u64, available: u64) -> MemoryPressure {
        if total == 0 {
            return MemoryPressure::Unknown;
        }
        if available < hard_emergency_reserve(total) {
            return self.transition(MemoryPressure::Emergency);
        }
        let thresholds = memory_reserve_thresholds(total);
        match self.state {
            MemoryPressure::Comfortable => {
                let target = if available < thresholds.emergency_bytes {
                    Some(MemoryPressure::Emergency)
                } else if available < thresholds.shrinking_bytes {
                    Some(MemoryPressure::Shrinking)
                } else {
                    None
                };
                self.enter_after_samples(target)
            }
            MemoryPressure::Shrinking => {
                if available < thresholds.emergency_bytes {
                    self.enter_after_samples(Some(MemoryPressure::Emergency))
                } else {
                    self.exit_after_samples(
                        available >= thresholds.shrinking_bytes.saturating_add(EXIT_MARGIN_BYTES),
                        MemoryPressure::Comfortable,
                    )
                }
            }
            MemoryPressure::Emergency => self.exit_after_samples(
                available >= thresholds.emergency_bytes.saturating_add(EXIT_MARGIN_BYTES),
                MemoryPressure::Shrinking,
            ),
            MemoryPressure::Unknown => self.transition(MemoryPressure::Comfortable),
        }
    }

    fn enter_after_samples(&mut self, target: Option<MemoryPressure>) -> MemoryPressure {
        let Some(target) = target else {
            self.reset_samples();
            return self.state;
        };
        self.count_toward(target, ENTER_SAMPLES)
    }

    fn exit_after_samples(&mut self, recovered: bool, target: MemoryPressure) -> MemoryPressure {
        if !recovered {
            self.reset_samples();
            return self.state;
        }
        self.count_toward(target, EXIT_SAMPLES)
    }

    fn count_toward(&mut self, target: MemoryPressure, required: u8) -> MemoryPressure {
        if self.pending == Some(target) {
            self.samples = self.samples.saturating_add(1);
        } else {
            self.pending = Some(target);
            self.samples = 1;
        }
        if self.samples >= required {
            return self.transition(target);
        }
        self.state
    }

    fn transition(&mut self, target: MemoryPressure) -> MemoryPressure {
        self.state = target;
        self.reset_samples();
        target
    }

    fn reset_samples(&mut self) {
        self.pending = None;
        self.samples = 0;
    }
}
