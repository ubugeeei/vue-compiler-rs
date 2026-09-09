mod tracker;

use vize_carton::{CompactString, FxHashMap};

pub const DEFINE_PROPS: &str = "defineProps";
pub const DEFINE_EMITS: &str = "defineEmits";
pub const DEFINE_EXPOSE: &str = "defineExpose";
pub const DEFINE_OPTIONS: &str = "defineOptions";
pub const DEFINE_SLOTS: &str = "defineSlots";
pub const DEFINE_MODEL: &str = "defineModel";
pub const WITH_DEFAULTS: &str = "withDefaults";
pub const DEFINE_ART: &str = "defineArt";
/// How a macro participates in the script setup compilation lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroLifecycle {
    /// The SFC compiler analyzes the call and emits the matching runtime code.
    AnalyzeAndErase,
    /// The SFC compiler rewrites the macro call into runtime code.
    Expand,
    /// The SFC compiler extracts the call as a separate artifact, then removes
    /// the top-level call from runtime output.
    ExtractAndErase,
}

impl MacroLifecycle {
    /// Whether calls with this lifecycle should be removed from setup runtime code.
    #[inline]
    pub const fn erases_from_runtime(self) -> bool {
        matches!(self, Self::AnalyzeAndErase | Self::ExtractAndErase)
    }

    /// Whether calls with this lifecycle produce a compiler artifact.
    #[inline]
    pub const fn produces_artifact(self) -> bool {
        matches!(self, Self::ExtractAndErase)
    }
}

/// Static metadata for a known compiler macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroDefinition {
    /// Macro call name.
    pub name: &'static str,
    /// How the macro participates in compilation.
    pub lifecycle: MacroLifecycle,
    /// Optional artifact kind emitted by the SFC compiler.
    pub artifact_kind: Option<&'static str>,
}

/// Built-in Vue compiler macros
pub static BUILTIN_MACROS: &[&str] = &[
    DEFINE_PROPS,
    DEFINE_EMITS,
    DEFINE_EXPOSE,
    DEFINE_OPTIONS,
    DEFINE_SLOTS,
    DEFINE_MODEL,
    WITH_DEFAULTS,
];

/// Known ecosystem macros that are compile-time only.
///
/// These are intentionally separate from Vue's built-in compiler macros:
/// Vize does not expand them into component runtime itself, but it may extract
/// artifacts for surrounding tooling before removing top-level calls.
pub static ECOSYSTEM_COMPILE_TIME_MACROS: &[MacroDefinition] = &[
    MacroDefinition {
        name: DEFINE_ART,
        lifecycle: MacroLifecycle::AnalyzeAndErase,
        artifact_kind: None,
    },
    MacroDefinition {
        name: "definePage",
        lifecycle: MacroLifecycle::ExtractAndErase,
        artifact_kind: Some("vue-router.definePage"),
    },
    MacroDefinition {
        name: "definePageMeta",
        lifecycle: MacroLifecycle::ExtractAndErase,
        artifact_kind: Some("nuxt.definePageMeta"),
    },
    MacroDefinition {
        name: "defineRouteRules",
        lifecycle: MacroLifecycle::ExtractAndErase,
        artifact_kind: Some("nuxt.defineRouteRules"),
    },
    MacroDefinition {
        name: "defineLazyHydrationComponent",
        lifecycle: MacroLifecycle::Expand,
        artifact_kind: None,
    },
];

/// Check if a name is a built-in compiler macro
#[inline]
pub fn is_builtin_macro(name: &str) -> bool {
    BUILTIN_MACROS.contains(&name)
}

/// Return the ecosystem macro definition for a name.
#[inline]
pub fn ecosystem_macro_definition(name: &str) -> Option<&'static MacroDefinition> {
    ECOSYSTEM_COMPILE_TIME_MACROS
        .iter()
        .find(|definition| definition.name == name)
}

/// Check if a name is a known ecosystem compile-time macro.
#[inline]
pub fn is_ecosystem_compile_time_macro(name: &str) -> bool {
    ecosystem_macro_definition(name).is_some()
}

/// Return the compile-time macro lifecycle for a name.
#[inline]
pub fn macro_lifecycle(name: &str) -> Option<MacroLifecycle> {
    if is_builtin_macro(name) {
        Some(MacroLifecycle::AnalyzeAndErase)
    } else {
        ecosystem_macro_definition(name).map(|definition| definition.lifecycle)
    }
}

/// Check if a name is compile-time only in script setup.
#[inline]
pub fn is_compile_time_macro(name: &str) -> bool {
    macro_lifecycle(name).is_some()
}

/// Check if a macro call should be removed from setup runtime output.
#[inline]
pub fn is_runtime_erased_macro(name: &str) -> bool {
    macro_lifecycle(name).is_some_and(MacroLifecycle::erases_from_runtime)
}

/// Return the artifact kind emitted for a macro name, when any.
#[inline]
pub fn macro_artifact_kind(name: &str) -> Option<&'static str> {
    ecosystem_macro_definition(name).and_then(|definition| definition.artifact_kind)
}

/// Check if a macro call should produce a compiler artifact.
#[inline]
pub fn is_artifact_macro(name: &str) -> bool {
    macro_lifecycle(name).is_some_and(MacroLifecycle::produces_artifact)
}

/// Names of macros that should not remain in setup runtime code.
#[inline]
pub fn runtime_erased_macro_names() -> impl Iterator<Item = &'static str> {
    BUILTIN_MACROS.iter().copied().chain(
        ECOSYSTEM_COMPILE_TIME_MACROS
            .iter()
            .filter(|definition| definition.lifecycle.erases_from_runtime())
            .map(|definition| definition.name),
    )
}

/// Names of macros that should produce compiler artifacts.
#[inline]
pub fn artifact_macro_names() -> impl Iterator<Item = &'static str> {
    ECOSYSTEM_COMPILE_TIME_MACROS
        .iter()
        .filter(|definition| definition.lifecycle.produces_artifact())
        .map(|definition| definition.name)
}

/// Unique identifier for a macro call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct MacroCallId(u32);

impl MacroCallId {
    #[inline(always)]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline(always)]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// Kind of macro
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MacroKind {
    DefineProps = 0,
    DefineEmits = 1,
    DefineExpose = 2,
    DefineOptions = 3,
    DefineSlots = 4,
    DefineModel = 5,
    WithDefaults = 6,
    Custom = 255,
}

impl MacroKind {
    #[inline]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            DEFINE_PROPS => Some(Self::DefineProps),
            DEFINE_EMITS => Some(Self::DefineEmits),
            DEFINE_EXPOSE => Some(Self::DefineExpose),
            DEFINE_OPTIONS => Some(Self::DefineOptions),
            DEFINE_SLOTS => Some(Self::DefineSlots),
            DEFINE_MODEL => Some(Self::DefineModel),
            WITH_DEFAULTS => Some(Self::WithDefaults),
            DEFINE_ART => Some(Self::Custom),
            _ => None,
        }
    }
}

/// A compiler macro call
#[derive(Debug, Clone)]
pub struct MacroCall {
    pub id: MacroCallId,
    pub name: CompactString,
    pub kind: MacroKind,
    pub start: u32,
    pub end: u32,
    pub runtime_args: Option<CompactString>,
    pub type_args: Option<CompactString>,
}

/// Props destructure binding info
#[derive(Debug, Clone)]
pub struct PropsDestructureBinding {
    pub local: CompactString,
    pub default: Option<CompactString>,
}

/// Props destructure bindings data
#[derive(Debug, Clone, Default)]
pub struct PropsDestructuredBindings {
    pub bindings: FxHashMap<CompactString, PropsDestructureBinding>,
    pub rest_id: Option<CompactString>,
}

impl PropsDestructuredBindings {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty() && self.rest_id.is_none()
    }

    #[inline]
    pub fn insert(
        &mut self,
        key: CompactString,
        local: CompactString,
        default: Option<CompactString>,
    ) {
        self.bindings
            .insert(key, PropsDestructureBinding { local, default });
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&PropsDestructureBinding> {
        self.bindings.get(key)
    }
}

/// Prop definition from defineProps
#[derive(Debug, Clone)]
pub struct PropDefinition {
    pub name: CompactString,
    pub prop_type: Option<CompactString>,
    pub required: bool,
    pub default_value: Option<CompactString>,
}

/// Emit definition from defineEmits
#[derive(Debug, Clone)]
pub struct EmitDefinition {
    pub name: CompactString,
    pub payload_type: Option<CompactString>,
}

/// An actual emit() call in the code
#[derive(Debug, Clone)]
pub struct EmitCall {
    /// Event name being emitted
    pub event_name: CompactString,
    /// Whether this is a dynamic emit (variable event name)
    pub is_dynamic: bool,
    /// Source start offset
    pub start: u32,
    /// Source end offset
    pub end: u32,
}

/// Model definition from defineModel
#[derive(Debug, Clone)]
pub struct ModelDefinition {
    pub name: CompactString,
    pub local_name: CompactString,
    pub model_type: Option<CompactString>,
    pub required: bool,
    pub default_value: Option<CompactString>,
}

/// Top-level await in script setup
#[derive(Debug, Clone)]
pub struct TopLevelAwait {
    pub start: u32,
    pub end: u32,
    pub expression: CompactString,
}

/// Expose definition from defineExpose
#[derive(Debug, Clone)]
pub struct ExposeDefinition {
    /// Exposed property name
    pub name: CompactString,
    /// Type of the exposed property (if known)
    pub expose_type: Option<CompactString>,
}

/// Slots definition from defineSlots
#[derive(Debug, Clone)]
pub struct SlotsDefinition {
    /// Slot name
    pub name: CompactString,
    /// Slot props type (if known)
    pub props_type: Option<CompactString>,
}

/// Musea art metadata from defineArt(component, options).
#[derive(Debug, Clone)]
pub struct ArtDefinition {
    pub component_name: CompactString,
    pub component_source: Option<CompactString>,
    /// Source string literal range including quotes, relative to the parsed script.
    pub component_source_span: Option<(u32, u32)>,
    /// Source string literal value range excluding quotes, relative to the parsed script.
    pub component_source_value_span: Option<(u32, u32)>,
    pub title: Option<CompactString>,
    pub description: Option<CompactString>,
    pub category: Option<CompactString>,
    pub tags: Vec<CompactString>,
    pub status: Option<CompactString>,
    pub order: Option<u32>,
}

/// Macro binding kind for props destructure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroBindingKind {
    /// Direct prop access
    Prop,
    /// Aliased prop
    PropAliased,
    /// Rest spread
    Rest,
}

/// Tracks compiler macros during analysis
#[derive(Default)]
pub struct MacroTracker {
    calls: Vec<MacroCall>,
    props: Vec<PropDefinition>,
    /// Written prop declarations, relative to the parsed script block.
    prop_declarations: FxHashMap<CompactString, (u32, u32)>,
    emits: Vec<EmitDefinition>,
    emit_declarations: FxHashMap<CompactString, (u32, u32)>,
    emit_calls: Vec<EmitCall>,
    models: Vec<ModelDefinition>,
    model_declarations: FxHashMap<CompactString, (u32, u32)>,
    model_modifier_types: FxHashMap<CompactString, CompactString>,
    /// Exposed properties from defineExpose
    exposes: Vec<ExposeDefinition>,
    /// Slots from defineSlots
    slots: Vec<SlotsDefinition>,
    /// Art metadata from defineArt
    art: Option<ArtDefinition>,
    props_destructure: Option<PropsDestructuredBindings>,
    top_level_awaits: Vec<TopLevelAwait>,
    next_id: u32,
    define_props_idx: Option<usize>,
    define_emits_idx: Option<usize>,
    define_expose_idx: Option<usize>,
    define_slots_idx: Option<usize>,
    define_art_idx: Option<usize>,
}

impl MacroTracker {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a macro call
    pub fn add_call(
        &mut self,
        name: impl Into<CompactString>,
        kind: MacroKind,
        start: u32,
        end: u32,
        runtime_args: Option<CompactString>,
        type_args: Option<CompactString>,
    ) -> MacroCallId {
        let name = name.into();
        let id = MacroCallId::new(self.next_id);
        self.next_id += 1;

        let idx = self.calls.len();

        // Cache macro indices for quick lookup
        match kind {
            MacroKind::DefineProps => self.define_props_idx = Some(idx),
            MacroKind::DefineEmits => self.define_emits_idx = Some(idx),
            MacroKind::DefineExpose => self.define_expose_idx = Some(idx),
            MacroKind::DefineSlots => self.define_slots_idx = Some(idx),
            _ => {}
        }
        if name.as_str() == DEFINE_ART {
            self.define_art_idx = Some(idx);
        }

        self.calls.push(MacroCall {
            id,
            name,
            kind,
            start,
            end,
            runtime_args,
            type_args,
        });

        id
    }

    /// Get all macro calls
    #[inline]
    pub fn all_calls(&self) -> &[MacroCall] {
        &self.calls
    }

    /// Get defineProps call (cached lookup)
    #[inline]
    pub fn define_props(&self) -> Option<&MacroCall> {
        self.define_props_idx.map(|idx| &self.calls[idx])
    }

    /// Get defineEmits call (cached lookup)
    #[inline]
    pub fn define_emits(&self) -> Option<&MacroCall> {
        self.define_emits_idx.map(|idx| &self.calls[idx])
    }

    /// Get defineExpose call (cached lookup)
    #[inline]
    pub fn define_expose(&self) -> Option<&MacroCall> {
        self.define_expose_idx.map(|idx| &self.calls[idx])
    }

    /// Get defineSlots call (cached lookup)
    #[inline]
    pub fn define_slots(&self) -> Option<&MacroCall> {
        self.define_slots_idx.map(|idx| &self.calls[idx])
    }

    /// Get defineArt call (cached lookup)
    #[inline]
    pub fn define_art_call(&self) -> Option<&MacroCall> {
        self.define_art_idx.map(|idx| &self.calls[idx])
    }

    /// Set defineArt metadata.
    #[inline]
    pub fn set_define_art(&mut self, art: ArtDefinition) {
        self.art = Some(art);
    }

    /// Get defineArt metadata.
    #[inline]
    pub fn define_art(&self) -> Option<&ArtDefinition> {
        self.art.as_ref()
    }

    /// Add an emit definition
    #[inline]
    pub fn add_emit(&mut self, emit: EmitDefinition) {
        self.emits.push(emit);
    }

    /// Get all emits
    #[inline]
    pub fn emits(&self) -> &[EmitDefinition] {
        &self.emits
    }

    /// Add an emit call (actual emit() invocation in code)
    #[inline]
    pub fn add_emit_call(
        &mut self,
        event_name: CompactString,
        is_dynamic: bool,
        start: u32,
        end: u32,
    ) {
        self.emit_calls.push(EmitCall {
            event_name,
            is_dynamic,
            start,
            end,
        });
    }

    /// Get all emit calls
    #[inline]
    pub fn emit_calls(&self) -> &[EmitCall] {
        &self.emit_calls
    }

    /// Check if an event is actually emitted (called)
    #[inline]
    pub fn is_event_emitted(&self, event_name: &str) -> bool {
        self.emit_calls
            .iter()
            .any(|c| c.event_name.as_str() == event_name && !c.is_dynamic)
    }

    /// Get emit calls for a specific event
    pub fn emit_calls_for_event<'a>(
        &'a self,
        event_name: &'a str,
    ) -> impl Iterator<Item = &'a EmitCall> + 'a {
        self.emit_calls
            .iter()
            .filter(move |c| c.event_name.as_str() == event_name)
    }

    /// Add an expose definition
    #[inline]
    pub fn add_expose(&mut self, expose: ExposeDefinition) {
        self.exposes.push(expose);
    }

    /// Get all exposes
    #[inline]
    pub fn exposes(&self) -> &[ExposeDefinition] {
        &self.exposes
    }

    /// Add a slot definition
    #[inline]
    pub fn add_slot(&mut self, slot: SlotsDefinition) {
        self.slots.push(slot);
    }

    /// Get all slots
    #[inline]
    pub fn slots(&self) -> &[SlotsDefinition] {
        &self.slots
    }

    /// Set props destructure
    #[inline]
    pub fn set_props_destructure(&mut self, destructure: PropsDestructuredBindings) {
        self.props_destructure = Some(destructure);
    }

    /// Get props destructure
    #[inline]
    pub fn props_destructure(&self) -> Option<&PropsDestructuredBindings> {
        self.props_destructure.as_ref()
    }

    /// Add top-level await
    #[inline]
    pub fn add_top_level_await(&mut self, expression: CompactString, start: u32, end: u32) {
        self.top_level_awaits.push(TopLevelAwait {
            start,
            end,
            expression,
        });
    }

    /// Check if there are top-level awaits
    #[inline]
    pub fn has_top_level_await(&self) -> bool {
        !self.top_level_awaits.is_empty()
    }

    /// Get all top-level awaits
    #[inline]
    pub fn top_level_awaits(&self) -> &[TopLevelAwait] {
        &self.top_level_awaits
    }

    /// Check if script is async (has top-level await)
    #[inline]
    pub fn is_async(&self) -> bool {
        self.has_top_level_await()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MacroKind, MacroLifecycle, MacroTracker, artifact_macro_names, is_artifact_macro,
        is_compile_time_macro, is_ecosystem_compile_time_macro, is_runtime_erased_macro,
        macro_artifact_kind, macro_lifecycle, runtime_erased_macro_names,
    };
    use vize_carton::CompactString;

    #[test]
    fn test_macro_lifecycle_classifies_builtin_and_ecosystem_macros() {
        assert_eq!(
            macro_lifecycle("defineProps"),
            Some(MacroLifecycle::AnalyzeAndErase)
        );
        assert_eq!(
            macro_lifecycle("definePage"),
            Some(MacroLifecycle::ExtractAndErase)
        );
        assert_eq!(
            macro_lifecycle("definePageMeta"),
            Some(MacroLifecycle::ExtractAndErase)
        );
        assert_eq!(
            macro_lifecycle("defineRouteRules"),
            Some(MacroLifecycle::ExtractAndErase)
        );
        assert_eq!(
            macro_lifecycle("defineLazyHydrationComponent"),
            Some(MacroLifecycle::Expand)
        );
        assert_eq!(
            macro_lifecycle("defineArt"),
            Some(MacroLifecycle::AnalyzeAndErase)
        );
        assert_eq!(macro_lifecycle("notAMacro"), None);
        assert_eq!(macro_lifecycle("useTemplateRef"), None);

        assert!(is_compile_time_macro("defineArt"));
        assert!(is_ecosystem_compile_time_macro("defineArt"));
        assert!(is_runtime_erased_macro("defineArt"));
        assert!(!is_artifact_macro("defineArt"));
        assert_eq!(macro_artifact_kind("defineArt"), None);
        assert!(is_compile_time_macro("definePage"));
        assert!(is_ecosystem_compile_time_macro("definePage"));
        assert!(is_runtime_erased_macro("definePage"));
        assert!(is_artifact_macro("definePage"));
        assert_eq!(
            macro_artifact_kind("definePage"),
            Some("vue-router.definePage")
        );
        assert!(is_compile_time_macro("definePageMeta"));
        assert!(is_ecosystem_compile_time_macro("definePageMeta"));
        assert!(is_runtime_erased_macro("definePageMeta"));
        assert!(is_artifact_macro("definePageMeta"));
        assert_eq!(
            macro_artifact_kind("definePageMeta"),
            Some("nuxt.definePageMeta")
        );
        assert!(is_compile_time_macro("defineRouteRules"));
        assert!(is_ecosystem_compile_time_macro("defineRouteRules"));
        assert!(is_runtime_erased_macro("defineRouteRules"));
        assert!(is_artifact_macro("defineRouteRules"));
        assert_eq!(
            macro_artifact_kind("defineRouteRules"),
            Some("nuxt.defineRouteRules")
        );
        assert!(is_compile_time_macro("defineLazyHydrationComponent"));
        assert!(is_ecosystem_compile_time_macro(
            "defineLazyHydrationComponent"
        ));
        assert!(!is_runtime_erased_macro("defineLazyHydrationComponent"));
        assert!(!is_artifact_macro("defineLazyHydrationComponent"));
        assert_eq!(macro_artifact_kind("defineLazyHydrationComponent"), None);
        assert!(runtime_erased_macro_names().any(|name| name == "definePage"));
        assert!(runtime_erased_macro_names().any(|name| name == "definePageMeta"));
        assert!(runtime_erased_macro_names().any(|name| name == "defineRouteRules"));
        assert!(!runtime_erased_macro_names().any(|name| name == "defineLazyHydrationComponent"));
        assert!(runtime_erased_macro_names().any(|name| name == "defineArt"));
        assert!(artifact_macro_names().any(|name| name == "definePage"));
        assert!(artifact_macro_names().any(|name| name == "definePageMeta"));
        assert!(artifact_macro_names().any(|name| name == "defineRouteRules"));
        assert!(!artifact_macro_names().any(|name| name == "defineLazyHydrationComponent"));
    }

    #[test]
    fn test_macro_tracker() {
        let mut tracker = MacroTracker::new();

        let id = tracker.add_call(
            "defineProps",
            MacroKind::DefineProps,
            0,
            20,
            None,
            Some(CompactString::new("{ msg: string }")),
        );

        assert_eq!(id.as_u32(), 0);
        assert!(tracker.define_props().is_some());
        assert!(tracker.define_emits().is_none());
    }

    #[test]
    fn test_top_level_await() {
        let mut tracker = MacroTracker::new();
        assert!(!tracker.is_async());

        tracker.add_top_level_await(CompactString::new("fetch('/api')"), 10, 30);
        assert!(tracker.is_async());
        assert_eq!(tracker.top_level_awaits().len(), 1);
    }
}
