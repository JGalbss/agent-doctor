//! Per-workspace configuration: `agent-doctor.toml` at the scan root lets a repo
//! pin the "clean enforcement" an agent must follow — which rules are on, at what
//! severity, and which tiers run by default — so the same standards apply no
//! matter who (or what) runs the linter. Everything is optional; an absent or
//! malformed file yields the defaults.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::diagnostics::{Diagnostic, Severity};

/// What a rule is set to in config: turned off, or pinned to a severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetting {
    Off,
    Pinned(Severity),
}

/// Default-on tiers, so a repo can permanently enable a tier without the flag.
/// `None` means "leave the CLI/default decision alone".
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct TierDefaults {
    pub agent: Option<bool>,
    pub agent_strict: Option<bool>,
    pub adopt: Option<bool>,
    pub react: Option<bool>,
}

/// `[design-system]` block: enable design-system enforcement by naming the
/// package that *is* the design system. The component catalog is auto-discovered
/// from that package's `exports`, so there's no manifest to maintain.
#[derive(Debug, Clone, Deserialize)]
pub struct DesignSystem {
    /// The design-system package (e.g. `@acme/ui`). Its `exports` map is the catalog.
    pub package: String,
    /// Import sources that should route through the design system instead
    /// (e.g. `@radix-ui/`, `class-variance-authority`). Prefix match.
    #[serde(default, rename = "forbid-import-prefixes")]
    pub forbid_import_prefixes: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Config {
    rules: HashMap<String, RuleSetting>,
    pub tiers: TierDefaults,
    pub design_system: Option<DesignSystem>,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    rules: HashMap<String, String>,
    #[serde(default)]
    tiers: TierDefaults,
    #[serde(default, rename = "design-system")]
    design_system: Option<DesignSystem>,
}

fn parse_setting(value: &str) -> Option<RuleSetting> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "false" | "none" => Some(RuleSetting::Off),
        "info" => Some(RuleSetting::Pinned(Severity::Info)),
        "warn" | "warning" => Some(RuleSetting::Pinned(Severity::Warn)),
        "error" => Some(RuleSetting::Pinned(Severity::Error)),
        _ => None,
    }
}

impl Config {
    /// Load `agent-doctor.toml` from `root`. Missing or unparsable → defaults.
    pub fn load(root: &Path) -> Config {
        let Ok(text) = std::fs::read_to_string(root.join("agent-doctor.toml")) else {
            return Config::default();
        };
        let Ok(raw) = toml::from_str::<RawConfig>(&text) else {
            return Config::default();
        };
        let rules = raw
            .rules
            .iter()
            .filter_map(|(id, value)| Some((id.clone(), parse_setting(value)?)))
            .collect();
        Config {
            rules,
            tiers: raw.tiers,
            design_system: raw.design_system,
        }
    }

    fn setting_for(&self, rule: &str) -> Option<RuleSetting> {
        self.rules.get(rule).copied()
    }

    /// Apply the configured overrides to a finished diagnostic set: drop rules set
    /// to `off`, and re-stamp the severity of pinned rules.
    pub fn apply(&self, diagnostics: &mut Vec<Diagnostic>) {
        if self.rules.is_empty() {
            return;
        }
        diagnostics.retain_mut(|diagnostic| match self.setting_for(diagnostic.rule) {
            Some(RuleSetting::Off) => false,
            Some(RuleSetting::Pinned(severity)) => {
                diagnostic.severity = severity;
                true
            }
            None => true,
        });
    }
}
