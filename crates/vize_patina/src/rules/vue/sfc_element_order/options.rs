use vize_s0::{String, ToCompactString, cstr};

static DEFAULT_HELP_ORDER: &str =
    "Recommended order: <script> and <template> (either order) -> <style>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SfcElementType<'a> {
    Script,
    ScriptSetup,
    Template,
    Style,
    Custom(&'a str),
}

impl SfcElementType<'_> {
    #[inline]
    pub(super) fn label(self) -> String {
        match self {
            Self::Script => "<script>".into(),
            Self::ScriptSetup => "<script setup>".into(),
            Self::Template => "<template>".into(),
            Self::Style => "<style>".into(),
            Self::Custom(name) => cstr!("<{name}>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SfcBlockSelector {
    Script,
    ScriptNormal,
    ScriptSetup,
    Template,
    Style,
    Custom(String),
}

impl SfcBlockSelector {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "script" => Some(Self::Script),
            "script:not([setup])" => Some(Self::ScriptNormal),
            "script[setup]" => Some(Self::ScriptSetup),
            "template" => Some(Self::Template),
            "style" => Some(Self::Style),
            custom if !custom.is_empty() => Some(Self::Custom(custom.into())),
            _ => None,
        }
    }

    fn matches(&self, kind: SfcElementType<'_>) -> bool {
        match (self, kind) {
            (Self::Script, SfcElementType::Script | SfcElementType::ScriptSetup) => true,
            (Self::ScriptNormal, SfcElementType::Script) => true,
            (Self::ScriptSetup, SfcElementType::ScriptSetup) => true,
            (Self::Template, SfcElementType::Template) => true,
            (Self::Style, SfcElementType::Style) => true,
            (Self::Custom(expected), SfcElementType::Custom(actual)) => expected.as_str() == actual,
            _ => false,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Script => "<script>".into(),
            Self::ScriptNormal => "<script>".into(),
            Self::ScriptSetup => "<script setup>".into(),
            Self::Template => "<template>".into(),
            Self::Style => "<style>".into(),
            Self::Custom(name) => cstr!("<{}>", name.as_str()),
        }
    }
}

/// One rank in `vue/sfc-element-order`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcElementOrderGroup {
    /// Selectors that may appear at this rank. Multiple selectors are
    /// interchangeable, mirroring eslint-plugin-vue's nested `order` groups.
    pub selectors: Vec<String>,
}

impl SfcElementOrderGroup {
    pub fn new(selectors: Vec<String>) -> Self {
        Self { selectors }
    }
}

/// Options for `vue/sfc-element-order`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcElementOrderOptions {
    pub order: Vec<SfcElementOrderGroup>,
}

impl Default for SfcElementOrderOptions {
    fn default() -> Self {
        Self {
            order: vec![
                SfcElementOrderGroup::new(vec!["script".into(), "template".into()]),
                SfcElementOrderGroup::new(vec!["style".into()]),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CompiledSfcElementOrder {
    groups: Vec<CompiledSfcElementOrderGroup>,
    help: String,
}

impl CompiledSfcElementOrder {
    pub(super) fn new(options: SfcElementOrderOptions) -> Self {
        let groups = options
            .order
            .iter()
            .filter_map(CompiledSfcElementOrderGroup::new)
            .collect::<Vec<_>>();
        let help = help_order(&groups);
        Self { groups, help }
    }

    pub(super) fn rank_for(&self, kind: SfcElementType<'_>) -> Option<usize> {
        self.groups.iter().position(|group| group.matches(kind))
    }

    pub(super) fn help(&self) -> &str {
        if self.groups.is_empty() {
            DEFAULT_HELP_ORDER
        } else {
            &self.help
        }
    }
}

impl Default for CompiledSfcElementOrder {
    fn default() -> Self {
        Self::new(SfcElementOrderOptions::default())
    }
}

#[derive(Debug, Clone)]
struct CompiledSfcElementOrderGroup {
    selectors: Vec<SfcBlockSelector>,
    label: String,
}

impl CompiledSfcElementOrderGroup {
    fn new(group: &SfcElementOrderGroup) -> Option<Self> {
        let selectors = group
            .selectors
            .iter()
            .filter_map(|selector| SfcBlockSelector::parse(selector.as_str()))
            .collect::<Vec<_>>();
        if selectors.is_empty() {
            return None;
        }
        let label = selectors
            .iter()
            .map(SfcBlockSelector::label)
            .collect::<Vec<_>>()
            .join(" / ")
            .to_compact_string();
        Some(Self { selectors, label })
    }

    fn matches(&self, kind: SfcElementType<'_>) -> bool {
        self.selectors.iter().any(|selector| selector.matches(kind))
    }
}

fn help_order(groups: &[CompiledSfcElementOrderGroup]) -> String {
    if groups.len() == 2
        && groups[0]
            .selectors
            .iter()
            .any(|selector| matches!(selector, SfcBlockSelector::Script))
        && groups[0]
            .selectors
            .iter()
            .any(|selector| matches!(selector, SfcBlockSelector::Template))
        && groups[1]
            .selectors
            .iter()
            .any(|selector| matches!(selector, SfcBlockSelector::Style))
    {
        return DEFAULT_HELP_ORDER.into();
    }

    cstr!(
        "Recommended order: {}",
        groups
            .iter()
            .map(|group| group.label.as_str())
            .collect::<Vec<_>>()
            .join(" -> ")
    )
}
