//! Rule trait and registry for lint rules.

use crate::context::LintContext;
use crate::diagnostic::Severity;
use crate::markup::MarkupRule;
use crate::preset::LintPreset;
use vize_relief::{DirectiveNode, ElementNode, ForNode, IfNode, InterpolationNode, RootNode};

/// Rule category for organization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    /// Essential rules (vue/essential) - prevent errors
    Essential,
    /// Strongly recommended rules (vue/strongly-recommended)
    StronglyRecommended,
    /// Recommended rules (vue/recommended)
    Recommended,
    /// Vapor mode specific rules
    Vapor,
    /// Musea (Art file / Storybook) specific rules
    Musea,
    /// Accessibility (a11y) rules
    Accessibility,
    /// HTML conformance rules
    HtmlConformance,
    /// Type-aware rules (require semantic analysis)
    TypeAware,
    /// Vue ecosystem integration rules (Nuxt, Vue Router, Pinia, vue-i18n, Void, and test utilities).
    Ecosystem,
}

/// Rule metadata
pub struct RuleMeta {
    /// Rule name (e.g., "vue/require-v-for-key")
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Rule category
    pub category: RuleCategory,
    /// Whether rule is auto-fixable
    pub fixable: bool,
    /// Default severity
    pub default_severity: Severity,
}

/// Rule trait for implementing lint rules
///
/// Rules implement visitor-like methods that are called during AST traversal.
/// Each method receives a mutable reference to LintContext for reporting diagnostics.
pub trait Rule: Send + Sync {
    /// Get rule metadata
    fn meta(&self) -> &'static RuleMeta;

    /// Project this rule onto the zero-copy markup IR, when it has a
    /// cross-backend [`MarkupRule`] implementation.
    ///
    /// Rules that also implement [`MarkupRule`] override this to return
    /// `Some(self)`, which lets the JSX/TSX lint path drive them directly over
    /// the borrow-based [`MarkupDocument`](crate::markup::MarkupDocument)
    /// projected from the OXC AST — no synthetic template reconstruction. Rules
    /// that return `None` (the default) have no JSX-capable entry point and are
    /// handled by the fallback lowering path instead.
    ///
    /// The returned reference borrows `self`, so the projection costs nothing.
    fn as_markup_rule(&self) -> Option<&dyn MarkupRule> {
        None
    }

    /// Whether this (markup-capable) rule's JSX/TSX equivalent only materializes
    /// after lowering, so it must run over the **lowered** markup IR rather than
    /// the OXC projection.
    ///
    /// Most migrated rules are element/attribute/binding-shaped and run on the
    /// zero-cost OXC projection directly. A few are structural: `v-for`'s JSX
    /// form is `items.map(…)`, a JS *expression* with no markup until lowering
    /// turns it into a `ForNode`/list scope. Such a rule returns `true` so the
    /// JSX path drives its [`MarkupRule`] hooks over the lowered relief AST (via
    /// the same markup visitor, so reporting stays unified and single), instead
    /// of the OXC AST where the list shape is absent.
    ///
    /// Ignored unless [`Self::as_markup_rule`] is `Some`. No effect on Vue
    /// templates, where the directive shape is present pre-lowering.
    fn jsx_needs_lowering(&self) -> bool {
        false
    }

    /// Run on the full SFC source before template extraction.
    #[allow(unused_variables)]
    fn run_on_sfc<'a>(&self, ctx: &mut LintContext<'a>) {}

    /// Run on template root node (called once per template)
    #[allow(unused_variables)]
    fn run_on_template<'a>(&self, ctx: &mut LintContext<'a>, root: &RootNode<'a>) {}

    /// Called when entering an element node
    #[allow(unused_variables)]
    fn enter_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {}

    /// Called when exiting an element node
    #[allow(unused_variables)]
    fn exit_element<'a>(&self, ctx: &mut LintContext<'a>, element: &ElementNode<'a>) {}

    /// Called for each directive on an element
    #[allow(unused_variables)]
    fn check_directive<'a>(
        &self,
        ctx: &mut LintContext<'a>,
        element: &ElementNode<'a>,
        directive: &DirectiveNode<'a>,
    ) {
    }

    /// Called for v-for nodes
    #[allow(unused_variables)]
    fn check_for<'a>(&self, ctx: &mut LintContext<'a>, for_node: &ForNode<'a>) {}

    /// Called for v-if nodes
    #[allow(unused_variables)]
    fn check_if<'a>(&self, ctx: &mut LintContext<'a>, if_node: &IfNode<'a>) {}

    /// Called for interpolation nodes {{ expr }}
    #[allow(unused_variables)]
    fn check_interpolation<'a>(
        &self,
        ctx: &mut LintContext<'a>,
        interpolation: &InterpolationNode<'a>,
    ) {
    }
}

/// Registry holding all enabled lint rules
pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
    rule_names: Vec<&'static str>,
    has_exit_element_rules: bool,
}

impl RuleRegistry {
    const ESSENTIAL_CAPACITY: usize = 32;
    const HAPPY_PATH_CAPACITY: usize = 90;
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            rule_names: Vec::new(),
            has_exit_element_rules: true,
        }
    }

    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        Self {
            rules: Vec::with_capacity(capacity),
            rule_names: Vec::with_capacity(capacity),
            has_exit_element_rules: false,
        }
    }

    /// Register a rule
    pub fn register(&mut self, rule: Box<dyn Rule>) {
        self.rule_names.push(rule.meta().name);
        self.rules.push(rule);
    }

    pub(crate) fn replace(&mut self, rule: Box<dyn Rule>) {
        let rule_name = rule.meta().name;
        if let Some(index) = self.rule_names.iter().position(|name| *name == rule_name) {
            self.rules[index] = rule;
            return;
        }
        self.register(rule);
    }

    pub fn add(&mut self, rule: Box<dyn Rule>) {
        self.register(rule);
    }

    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    pub fn rule_names(&self) -> &[&'static str] {
        &self.rule_names
    }

    pub fn has_exit_element_rules(&self) -> bool {
        self.has_exit_element_rules
    }

    pub(crate) fn mark_has_exit_element_rules(&mut self) {
        self.has_exit_element_rules = true;
    }

    pub fn has_rule(&self, name: &str) -> bool {
        self.rule_names.contains(&name)
    }

    /// Whether any registered rule exposes a [`MarkupRule`] projection (i.e. has
    /// a JSX-capable IR entry point via [`Rule::as_markup_rule`]).
    ///
    /// Lets the JSX lint path skip building the markup IR entirely when the
    /// active rule set has nothing to run over it.
    pub fn has_markup_rules(&self) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.as_markup_rule().is_some())
    }

    /// Create a registry for a named preset.
    pub fn with_preset(preset: LintPreset) -> Self {
        match preset {
            LintPreset::HappyPath => Self::with_happy_path(),
            LintPreset::Opinionated => Self::with_opinionated(),
            LintPreset::Essential => Self::with_essential(),
            LintPreset::Incremental => Self::with_incremental(),
            LintPreset::Ecosystem => Self::with_ecosystem(),
            LintPreset::Nuxt => Self::with_nuxt(),
        }
    }

    /// Create an empty registry for host-driven, rule-by-rule adoption.
    pub fn with_incremental() -> Self {
        Self::with_capacity(0)
    }

    /// Create a registry containing rules that are exposed only for explicit
    /// opt-in. These rules do not belong to any preset.
    pub fn with_opt_in_rules() -> Self {
        let mut registry = Self::with_capacity(16);
        crate::rules::ecosystem::register_opt_in(&mut registry);
        crate::rules::petite_vue::register_opt_in(&mut registry);
        crate::rules::vue::register_opt_in(&mut registry);
        registry
    }

    /// Register explicit opt-in rules into an existing registry.
    pub fn register_opt_in_rules(&mut self) {
        crate::rules::ecosystem::register_opt_in(self);
        crate::rules::petite_vue::register_opt_in(self);
        crate::rules::vue::register_opt_in(self);
    }

    /// Create the default happy-path registry.
    ///
    /// This focuses on broad correctness, security, and accessibility checks
    /// without enforcing stronger stylistic or framework-specific conventions.
    pub fn with_happy_path() -> Self {
        let mut registry = Self::with_capacity(Self::HAPPY_PATH_CAPACITY);
        // Vue correctness rules.
        registry.register(Box::new(crate::rules::vue::RequireVForKey));
        registry.register(Box::new(crate::rules::vue::ValidVFor));
        registry.register(Box::new(crate::rules::vue::NoUseVIfWithVFor));
        registry.register(Box::new(crate::rules::vue::NoUnusedVars::default()));
        registry.register(Box::new(crate::rules::vue::NoDuplicateAttributes::default()));
        registry.register(Box::new(crate::rules::vue::NoTemplateKey));
        registry.register(Box::new(crate::rules::vue::NoTextareaMustache));
        registry.register(Box::new(crate::rules::vue::ValidVElse));
        registry.register(Box::new(crate::rules::vue::ValidVIf));
        registry.register(Box::new(crate::rules::vue::ValidVOn));
        registry.register(Box::new(crate::rules::vue::ValidVBind));
        registry.register(Box::new(crate::rules::vue::ValidVModel));
        registry.register(Box::new(crate::rules::vue::ValidVShow));
        registry.register(Box::new(crate::rules::vue::NoDupeVElseIf));
        registry.register(Box::new(
            crate::rules::vue::NoReservedComponentNames::default(),
        ));
        registry.register(Box::new(crate::rules::vue::ComponentDefinitionNameCasing));
        registry.register(Box::new(crate::rules::vue::HtmlQuotes::default()));
        registry.register(Box::new(
            crate::rules::vue::MustacheInterpolationSpacing::default(),
        ));
        registry.register(Box::new(crate::rules::vue::NoLoneTemplate));
        registry.register(Box::new(crate::rules::vue::NoMultiSpaces::default()));
        registry.register(Box::new(crate::rules::vue::PropNameCasing::default()));
        registry.register(Box::new(crate::rules::vue::VOnStyle::default()));
        registry.register(Box::new(crate::rules::vue::VSlotStyle::default()));
        registry.register(Box::new(crate::rules::vue::ValidVSlot));
        registry.register(Box::new(crate::rules::vue::NoChildContent));
        registry.register(Box::new(crate::rules::vue::ValidAttributeName));
        registry.register(Box::new(crate::rules::vue::AttributeHyphenation::default()));
        registry.register(Box::new(crate::rules::vue::AttributeOrder));
        registry.register(Box::new(crate::rules::vue::NoVTextVHtmlOnComponent));
        registry.register(Box::new(crate::rules::vue::RequireComponentIs));
        registry.register(Box::new(crate::rules::vue::RequireScopedStyle));
        registry.register(Box::new(crate::rules::vue::SfcElementOrder::default()));
        registry.register(Box::new(crate::rules::vue::SingleStyleBlock));
        registry.register(Box::new(crate::rules::vue::NoUselessTemplateAttributes));
        registry.register(Box::new(crate::rules::vue::NoDeprecatedSlotAttribute));
        crate::rules::vue::register_valid_directives(&mut registry);
        registry.register(Box::new(crate::rules::vapor::NoVueLifecycleEvents));
        crate::rules::vue::register_security(&mut registry);
        // Accessibility rules with broadly applicable guidance.
        registry.register(Box::new(crate::rules::a11y::ImgAlt));
        registry.register(Box::new(crate::rules::a11y::AnchorHasContent));
        registry.register(Box::new(crate::rules::a11y::HeadingHasContent));
        registry.register(Box::new(crate::rules::a11y::IframeHasTitle));
        registry.register(Box::new(crate::rules::a11y::NoDistractingElements));
        registry.register(Box::new(crate::rules::a11y::NoIForIcon));
        registry.register(Box::new(crate::rules::a11y::TabindexNoPositive));
        registry.register(Box::new(crate::rules::a11y::ClickEventsHaveKeyEvents));
        registry.register(Box::new(crate::rules::a11y::FormControlHasLabel));
        registry.register(Box::new(crate::rules::a11y::AriaProps));
        registry.register(Box::new(crate::rules::a11y::AriaRole::default()));
        registry.register(Box::new(crate::rules::a11y::NoAriaHiddenOnFocusable));
        registry.register(Box::new(crate::rules::a11y::NoAccessKey));
        registry.register(Box::new(crate::rules::a11y::NoAutofocus));
        registry.register(Box::new(crate::rules::a11y::NoRolePresentationOnFocusable));
        registry.register(Box::new(crate::rules::a11y::AriaUnsupportedElements));
        registry.register(Box::new(crate::rules::a11y::NoRedundantRoles));
        registry.register(Box::new(crate::rules::a11y::MouseEventsHaveKeyEvents));
        registry.register(Box::new(crate::rules::a11y::AltText));
        registry.register(Box::new(crate::rules::a11y::AnchorIsValid));
        registry.register(Box::new(crate::rules::a11y::LabelHasFor));
        registry.register(Box::new(crate::rules::a11y::InteractiveSupportsFocus));
        registry.register(Box::new(crate::rules::a11y::RoleHasRequiredAriaProps));
        registry.register(Box::new(crate::rules::a11y::MediaHasCaption));
        registry.register(Box::new(crate::rules::a11y::NoStaticElementInteractions));
        registry.register(Box::new(crate::rules::a11y::NoReferToNonExistentId));
        registry.register(Box::new(crate::rules::vue::PermittedContents));

        // HTML conformance rules.
        registry.register(Box::new(crate::rules::html::DeprecatedElement));
        registry.register(Box::new(crate::rules::html::DeprecatedAttr));
        registry.register(Box::new(crate::rules::html::NoConsecutiveBr));
        registry.register(Box::new(crate::rules::html::IdDuplication));
        registry.register(Box::new(crate::rules::html::NoDuplicateDt));
        registry.register(Box::new(crate::rules::html::NoEmptyPalpableContent));
        registry.register(Box::new(crate::rules::html::RequireDatetime));

        // SSR rules.
        registry.register(Box::new(crate::rules::ssr::NoBrowserGlobalsInSsr));
        registry.register(Box::new(crate::rules::ssr::NoHydrationMismatch));

        // Semantic analysis rules.
        registry.register(Box::new(crate::rules::vue::NoUnusedComponents::default()));
        registry.register(Box::new(crate::rules::vue::NoMutatingProps::default()));
        registry.register(Box::new(crate::rules::vue::NoUnusedProperties::default()));
        #[cfg(not(target_arch = "wasm32"))]
        registry.register(Box::new(
            crate::rules::type_aware::RequireTypedProps::default(),
        ));
        #[cfg(not(target_arch = "wasm32"))]
        registry.register(Box::new(
            crate::rules::type_aware::RequireTypedEmits::default(),
        ));

        registry
    }

    /// Backward-compatible alias for the default preset.
    pub fn with_recommended() -> Self {
        Self::with_happy_path()
    }

    /// Create registry with only essential rules.
    pub fn with_essential() -> Self {
        let mut registry = Self::with_capacity(Self::ESSENTIAL_CAPACITY);
        // Vue Essential Rules only
        registry.register(Box::new(crate::rules::vue::RequireVForKey));
        registry.register(Box::new(crate::rules::vue::ValidVFor));
        registry.register(Box::new(crate::rules::vue::NoUseVIfWithVFor));
        registry.register(Box::new(crate::rules::vue::NoUnusedVars::default()));
        registry.register(Box::new(crate::rules::vue::NoMutatingProps::default()));
        registry.register(Box::new(crate::rules::vue::NoDuplicateAttributes::default()));
        registry.register(Box::new(crate::rules::vue::NoTemplateKey));
        registry.register(Box::new(crate::rules::vue::NoTextareaMustache));
        registry.register(Box::new(crate::rules::vue::ValidVElse));
        registry.register(Box::new(crate::rules::vue::ValidVIf));
        registry.register(Box::new(crate::rules::vue::ValidVOn));
        registry.register(Box::new(crate::rules::vue::ValidVBind));
        registry.register(Box::new(crate::rules::vue::ValidVModel));
        registry.register(Box::new(crate::rules::vue::ValidVShow));
        registry.register(Box::new(crate::rules::vue::NoDupeVElseIf));
        registry.register(Box::new(
            crate::rules::vue::NoReservedComponentNames::default(),
        ));
        registry.register(Box::new(crate::rules::vue::ValidVSlot));
        registry.register(Box::new(
            crate::rules::vue::MultiWordComponentNames::default(),
        ));
        registry.register(Box::new(crate::rules::vue::NoChildContent));
        registry.register(Box::new(crate::rules::vue::ValidAttributeName));
        registry.register(Box::new(crate::rules::vue::NoVTextVHtmlOnComponent));
        registry.register(Box::new(crate::rules::vue::RequireComponentIs));
        registry.register(Box::new(crate::rules::vue::NoUselessTemplateAttributes));
        registry.register(Box::new(crate::rules::vue::NoDeprecatedSlotAttribute));
        crate::rules::vue::register_valid_directives(&mut registry);
        registry.register(Box::new(crate::rules::vue::UseVOnExact));

        // Security Rules
        registry.register(Box::new(crate::rules::vue::NoVHtml));
        registry.register(Box::new(crate::rules::vue::NoUnsafeUrl));

        // HTML Conformance (essential)
        registry.register(Box::new(crate::rules::html::IdDuplication));

        registry
    }

    /// Create registry with the strongest built-in preset enabled.
    pub fn with_opinionated() -> Self {
        let mut registry = Self::with_happy_path();
        crate::rules::opinionated::register(&mut registry);

        registry
    }

    /// Create registry with broad defaults plus ecosystem integration rules.
    pub fn with_ecosystem() -> Self {
        let mut registry = Self::with_happy_path();
        crate::rules::ecosystem::register(&mut registry);

        registry
    }

    /// Create registry with all available rules (including opt-in).
    pub fn with_all() -> Self {
        let mut registry = Self::with_opinionated();
        crate::rules::ecosystem::register_all(&mut registry);

        registry
    }

    /// Create registry with Nuxt-friendly rules (auto-imports enabled).
    pub fn with_nuxt() -> Self {
        let mut registry = Self::with_happy_path();
        crate::rules::register_nuxt(&mut registry);

        registry
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::with_preset(LintPreset::default())
    }
}
