use serde::{Deserialize, Serialize};

/// Options for `vue/no-mutating-props`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct NoMutatingPropsOptions {
    pub shallow_only: bool,
}
