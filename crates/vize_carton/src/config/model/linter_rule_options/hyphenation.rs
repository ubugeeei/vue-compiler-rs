use serde::{Deserialize, Serialize};

/// Hyphenation style used by Vue template naming rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HyphenationStyle {
    Always,
    Never,
}
