use crate::database::structures::ReviveEntry;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PricingType {
    #[default]
    Legacy,
    External,
    InterAlliance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviveClass {
    Success,
    FailedCounted,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviveCounts {
    pub successful: u64,
    pub failed_counted: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceBreakdown {
    pub base: u64,
    pub final_with_markup: u64,
    pub markup_percent: i64,
}

impl PricingType {
    fn success_rate(self) -> u64 {
        match self {
            Self::Legacy => 900_000,
            Self::External => 1_000_000,
            Self::InterAlliance => 800_000,
        }
    }

    fn failed_rate(self) -> u64 {
        match self {
            Self::Legacy => 1_000_000,
            Self::External => 750_000,
            Self::InterAlliance => 550_000,
        }
    }

    pub fn default_faction_cut(self) -> i64 {
        match self {
            Self::Legacy | Self::External => 10,
            Self::InterAlliance => 0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::External => "external",
            Self::InterAlliance => "inter_alliance",
        }
    }

    pub fn calculate(self, counts: ReviveCounts, faction_cut: i64) -> PriceBreakdown {
        let base = counts.successful * self.success_rate()
            + counts.failed_counted * self.failed_rate();
        let final_with_markup =
            (base as f64 * (1.0 + faction_cut as f64 / 100.0)).round() as u64;

        PriceBreakdown {
            base,
            final_with_markup,
            markup_percent: faction_cut,
        }
    }
}

pub fn classify_revive(revive: &ReviveEntry, min_chance: u64) -> ReviveClass {
    if revive.result == "success" {
        ReviveClass::Success
    } else if revive.result == "failure" && revive.chance >= min_chance as f32 {
        ReviveClass::FailedCounted
    } else {
        ReviveClass::Ignored
    }
}
