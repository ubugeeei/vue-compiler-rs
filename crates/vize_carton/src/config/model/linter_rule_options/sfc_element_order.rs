use serde::{Deserialize, Serialize};

use crate::String;

/// One order rank for `vue/sfc-element-order`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SfcElementOrderGroup {
    Single(String),
    Any(Vec<String>),
}

impl SfcElementOrderGroup {
    pub fn selectors(&self) -> Vec<String> {
        match self {
            Self::Single(selector) => vec![selector.clone()],
            Self::Any(selectors) => selectors.clone(),
        }
    }
}

/// Options for `vue/sfc-element-order`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SfcElementOrderOptions {
    pub order: Vec<SfcElementOrderGroup>,
}

impl Default for SfcElementOrderOptions {
    fn default() -> Self {
        Self {
            order: vec![
                SfcElementOrderGroup::Any(vec!["script".into(), "template".into()]),
                SfcElementOrderGroup::Single("style".into()),
            ],
        }
    }
}
