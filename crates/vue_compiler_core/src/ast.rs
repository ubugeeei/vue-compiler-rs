//! Vue template AST node types.
//!
//! This module defines the AST (Abstract Syntax Tree) for Vue templates.
//! All AST nodes are allocated in a bumpalo arena for efficient memory management
//! and zero-copy transfer to JavaScript.

use serde::{Deserialize, Serialize};
use vue_allocator::{Box, Bump, String, Vec};
use vue_shared::PatchFlags;

/// Node type discriminant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NodeType {
    Root = 0,
    Element = 1,
    Text = 2,
    Comment = 3,
    SimpleExpression = 4,
    Interpolation = 5,
    Attribute = 6,
    Directive = 7,
    CompoundExpression = 8,
    If = 9,
    IfBranch = 10,
    For = 11,
    TextCall = 12,
    // Codegen nodes
    VNodeCall = 13,
    JsCallExpression = 14,
    JsObjectExpression = 15,
    JsProperty = 16,
    JsArrayExpression = 17,
    JsFunctionExpression = 18,
    JsConditionalExpression = 19,
    JsCacheExpression = 20,
    // SSR codegen nodes
    JsBlockStatement = 21,
    JsTemplateLiteral = 22,
    JsIfStatement = 23,
    JsAssignmentExpression = 24,
    JsSequenceExpression = 25,
    JsReturnStatement = 26,
}

/// Element type discriminant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum ElementType {
    #[default]
    Element = 0,
    Component = 1,
    Slot = 2,
    Template = 3,
}

/// Namespace for elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum Namespace {
    #[default]
    Html = 0,
    Svg = 1,
    MathMl = 2,
}

/// Constant type levels for static analysis
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[repr(u8)]
pub enum ConstantType {
    #[default]
    NotConstant = 0,
    CanSkipPatch = 1,
    CanCache = 2,
    CanStringify = 3,
}

/// Source position in the template
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Position {
    /// Byte offset from start of file
    pub offset: u32,
    /// 1-indexed line number
    pub line: u32,
    /// 1-indexed column number
    pub column: u32,
}

impl Position {
    pub const fn new(offset: u32, line: u32, column: u32) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }
}

/// Source location span [start, end)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub start: Position,
    pub end: Position,
    pub source: String,
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self::STUB
    }
}

/// Static stub location for returning references
static STUB_LOCATION: SourceLocation = SourceLocation {
    start: Position {
        offset: 0,
        line: 1,
        column: 1,
    },
    end: Position {
        offset: 0,
        line: 1,
        column: 1,
    },
    source: String::const_new(""),
};

impl SourceLocation {
    /// Stub location for generated nodes
    pub const STUB: Self = Self {
        start: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        source: String::const_new(""),
    };

    pub fn new(start: Position, end: Position, source: impl Into<String>) -> Self {
        Self {
            start,
            end,
            source: source.into(),
        }
    }
}

/// Root AST node
#[derive(Debug)]
pub struct RootNode<'a> {
    pub children: Vec<'a, TemplateChildNode<'a>>,
    pub helpers: Vec<'a, RuntimeHelper>,
    pub components: Vec<'a, String>,
    pub directives: Vec<'a, String>,
    pub hoists: Vec<'a, Option<JsChildNode<'a>>>,
    pub imports: Vec<'a, ImportItem<'a>>,
    pub cached: Vec<'a, Option<Box<'a, CacheExpression<'a>>>>,
    pub temps: u32,
    pub source: String,
    pub loc: SourceLocation,
    pub codegen_node: Option<CodegenNode<'a>>,
    pub transformed: bool,
}

impl<'a> RootNode<'a> {
    pub fn new(allocator: &'a Bump, source: impl Into<String>) -> Self {
        Self {
            children: Vec::new_in(allocator),
            helpers: Vec::new_in(allocator),
            components: Vec::new_in(allocator),
            directives: Vec::new_in(allocator),
            hoists: Vec::new_in(allocator),
            imports: Vec::new_in(allocator),
            cached: Vec::new_in(allocator),
            temps: 0,
            source: source.into(),
            loc: SourceLocation::STUB,
            codegen_node: None,
            transformed: false,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Root
    }
}

/// Import item for code generation
#[derive(Debug)]
pub struct ImportItem<'a> {
    pub exp: Box<'a, SimpleExpressionNode<'a>>,
    pub path: String,
}

/// Runtime helper symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum RuntimeHelper {
    Fragment,
    Teleport,
    Suspense,
    KeepAlive,
    BaseTransition,
    Transition,
    TransitionGroup,
    OpenBlock,
    CreateBlock,
    CreateElementBlock,
    CreateVNode,
    CreateElementVNode,
    CreateComment,
    CreateText,
    CreateStatic,
    ResolveComponent,
    ResolveDynamicComponent,
    ResolveDirective,
    ResolveFilter,
    WithDirectives,
    RenderList,
    RenderSlot,
    CreateSlots,
    ToDisplayString,
    MergeProps,
    NormalizeClass,
    NormalizeStyle,
    NormalizeProps,
    GuardReactiveProps,
    ToHandlers,
    Camelize,
    Capitalize,
    ToHandlerKey,
    SetBlockTracking,
    PushScopeId,
    PopScopeId,
    WithCtx,
    Unref,
    IsRef,
    WithMemo,
    IsMemoSame,
    VShow,
    VModelText,
    VModelCheckbox,
    VModelRadio,
    VModelSelect,
    VModelDynamic,
    WithModifiers,
    WithKeys,
}

impl RuntimeHelper {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fragment => "Fragment",
            Self::Teleport => "Teleport",
            Self::Suspense => "Suspense",
            Self::KeepAlive => "KeepAlive",
            Self::BaseTransition => "BaseTransition",
            Self::Transition => "Transition",
            Self::TransitionGroup => "TransitionGroup",
            Self::OpenBlock => "openBlock",
            Self::CreateBlock => "createBlock",
            Self::CreateElementBlock => "createElementBlock",
            Self::CreateVNode => "createVNode",
            Self::CreateElementVNode => "createElementVNode",
            Self::CreateComment => "createCommentVNode",
            Self::CreateText => "createTextVNode",
            Self::CreateStatic => "createStaticVNode",
            Self::ResolveComponent => "resolveComponent",
            Self::ResolveDynamicComponent => "resolveDynamicComponent",
            Self::ResolveDirective => "resolveDirective",
            Self::ResolveFilter => "resolveFilter",
            Self::WithDirectives => "withDirectives",
            Self::RenderList => "renderList",
            Self::RenderSlot => "renderSlot",
            Self::CreateSlots => "createSlots",
            Self::ToDisplayString => "toDisplayString",
            Self::MergeProps => "mergeProps",
            Self::NormalizeClass => "normalizeClass",
            Self::NormalizeStyle => "normalizeStyle",
            Self::NormalizeProps => "normalizeProps",
            Self::GuardReactiveProps => "guardReactiveProps",
            Self::ToHandlers => "toHandlers",
            Self::Camelize => "camelize",
            Self::Capitalize => "capitalize",
            Self::ToHandlerKey => "toHandlerKey",
            Self::SetBlockTracking => "setBlockTracking",
            Self::PushScopeId => "pushScopeId",
            Self::PopScopeId => "popScopeId",
            Self::WithCtx => "withCtx",
            Self::Unref => "unref",
            Self::IsRef => "isRef",
            Self::WithMemo => "withMemo",
            Self::IsMemoSame => "isMemoSame",
            Self::VShow => "vShow",
            Self::VModelText => "vModelText",
            Self::VModelCheckbox => "vModelCheckbox",
            Self::VModelRadio => "vModelRadio",
            Self::VModelSelect => "vModelSelect",
            Self::VModelDynamic => "vModelDynamic",
            Self::WithModifiers => "withModifiers",
            Self::WithKeys => "withKeys",
        }
    }
}

// ============================================================================
// Template Nodes
// ============================================================================

/// All template child node types
#[derive(Debug)]
pub enum TemplateChildNode<'a> {
    Element(Box<'a, ElementNode<'a>>),
    Text(Box<'a, TextNode>),
    Comment(Box<'a, CommentNode>),
    Interpolation(Box<'a, InterpolationNode<'a>>),
    If(Box<'a, IfNode<'a>>),
    IfBranch(Box<'a, IfBranchNode<'a>>),
    For(Box<'a, ForNode<'a>>),
    TextCall(Box<'a, TextCallNode<'a>>),
    CompoundExpression(Box<'a, CompoundExpressionNode<'a>>),
    /// Reference to a hoisted node (index into root.hoists array)
    Hoisted(usize),
}

impl<'a> TemplateChildNode<'a> {
    pub fn node_type(&self) -> NodeType {
        match self {
            Self::Element(_) => NodeType::Element,
            Self::Text(_) => NodeType::Text,
            Self::Comment(_) => NodeType::Comment,
            Self::Interpolation(_) => NodeType::Interpolation,
            Self::If(_) => NodeType::If,
            Self::IfBranch(_) => NodeType::IfBranch,
            Self::For(_) => NodeType::For,
            Self::TextCall(_) => NodeType::TextCall,
            Self::CompoundExpression(_) => NodeType::CompoundExpression,
            Self::Hoisted(_) => NodeType::SimpleExpression, // Hoisted refs are like expressions
        }
    }

    pub fn loc(&self) -> &SourceLocation {
        match self {
            Self::Element(n) => &n.loc,
            Self::Text(n) => &n.loc,
            Self::Comment(n) => &n.loc,
            Self::Interpolation(n) => &n.loc,
            Self::If(n) => &n.loc,
            Self::IfBranch(n) => &n.loc,
            Self::For(n) => &n.loc,
            Self::TextCall(n) => &n.loc,
            Self::CompoundExpression(n) => &n.loc,
            Self::Hoisted(_) => &STUB_LOCATION, // Hoisted refs don't have a real location
        }
    }
}

/// Element node
#[derive(Debug)]
pub struct ElementNode<'a> {
    pub ns: Namespace,
    pub tag: String,
    pub tag_type: ElementType,
    pub props: Vec<'a, PropNode<'a>>,
    pub children: Vec<'a, TemplateChildNode<'a>>,
    pub is_self_closing: bool,
    pub loc: SourceLocation,
    pub inner_loc: Option<SourceLocation>,
    pub codegen_node: Option<ElementCodegenNode<'a>>,
    /// If props are hoisted, this is the index into the hoists array (1-based for _hoisted_N)
    pub hoisted_props_index: Option<usize>,
}

impl<'a> ElementNode<'a> {
    pub fn new(allocator: &'a Bump, tag: impl Into<String>, loc: SourceLocation) -> Self {
        Self {
            ns: Namespace::Html,
            tag: tag.into(),
            tag_type: ElementType::Element,
            props: Vec::new_in(allocator),
            children: Vec::new_in(allocator),
            is_self_closing: false,
            loc,
            inner_loc: None,
            codegen_node: None,
            hoisted_props_index: None,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Element
    }
}

/// Element codegen node (VNodeCall, SimpleExpression, CacheExpression, etc.)
#[derive(Debug)]
pub enum ElementCodegenNode<'a> {
    VNodeCall(Box<'a, VNodeCall<'a>>),
    SimpleExpression(Box<'a, SimpleExpressionNode<'a>>),
    CacheExpression(Box<'a, CacheExpression<'a>>),
}

/// Prop node (attribute or directive)
#[derive(Debug)]
pub enum PropNode<'a> {
    Attribute(Box<'a, AttributeNode>),
    Directive(Box<'a, DirectiveNode<'a>>),
}

impl<'a> PropNode<'a> {
    pub fn loc(&self) -> &SourceLocation {
        match self {
            Self::Attribute(n) => &n.loc,
            Self::Directive(n) => &n.loc,
        }
    }
}

/// Attribute node
#[derive(Debug)]
pub struct AttributeNode {
    pub name: String,
    pub name_loc: SourceLocation,
    pub value: Option<TextNode>,
    pub loc: SourceLocation,
}

impl AttributeNode {
    pub fn new(name: impl Into<String>, loc: SourceLocation) -> Self {
        Self {
            name: name.into(),
            name_loc: loc.clone(),
            value: None,
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Attribute
    }
}

/// Directive node (v-if, v-for, v-bind, etc.)
#[derive(Debug)]
pub struct DirectiveNode<'a> {
    /// Normalized directive name without prefix (e.g., "if", "for", "bind")
    pub name: String,
    /// Raw attribute name including shorthand (e.g., "@click", ":class")
    pub raw_name: Option<String>,
    /// Directive expression
    pub exp: Option<ExpressionNode<'a>>,
    /// Directive argument (e.g., "click" in @click)
    pub arg: Option<ExpressionNode<'a>>,
    /// Directive modifiers (e.g., ["stop", "prevent"] in @click.stop.prevent)
    pub modifiers: Vec<'a, SimpleExpressionNode<'a>>,
    /// Parsed result for v-for
    pub for_parse_result: Option<ForParseResult<'a>>,
    pub loc: SourceLocation,
}

impl<'a> DirectiveNode<'a> {
    pub fn new(allocator: &'a Bump, name: impl Into<String>, loc: SourceLocation) -> Self {
        Self {
            name: name.into(),
            raw_name: None,
            exp: None,
            arg: None,
            modifiers: Vec::new_in(allocator),
            for_parse_result: None,
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Directive
    }
}

/// Text node
#[derive(Debug)]
pub struct TextNode {
    pub content: String,
    pub loc: SourceLocation,
}

impl TextNode {
    pub fn new(content: impl Into<String>, loc: SourceLocation) -> Self {
        Self {
            content: content.into(),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Text
    }
}

/// Comment node
#[derive(Debug)]
pub struct CommentNode {
    pub content: String,
    pub loc: SourceLocation,
}

impl CommentNode {
    pub fn new(content: impl Into<String>, loc: SourceLocation) -> Self {
        Self {
            content: content.into(),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Comment
    }
}

/// Interpolation node ({{ expr }})
#[derive(Debug)]
pub struct InterpolationNode<'a> {
    pub content: ExpressionNode<'a>,
    pub loc: SourceLocation,
}

impl<'a> InterpolationNode<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::Interpolation
    }
}

// ============================================================================
// Expression Nodes
// ============================================================================

/// Expression node types
#[derive(Debug)]
pub enum ExpressionNode<'a> {
    Simple(Box<'a, SimpleExpressionNode<'a>>),
    Compound(Box<'a, CompoundExpressionNode<'a>>),
}

impl<'a> ExpressionNode<'a> {
    pub fn loc(&self) -> &SourceLocation {
        match self {
            Self::Simple(n) => &n.loc,
            Self::Compound(n) => &n.loc,
        }
    }
}

/// Simple expression node
#[derive(Debug)]
pub struct SimpleExpressionNode<'a> {
    pub content: String,
    pub is_static: bool,
    pub const_type: ConstantType,
    pub loc: SourceLocation,
    /// Parsed JavaScript AST (None = simple identifier, Some = parsed expression)
    pub js_ast: Option<JsExpression<'a>>,
    /// Hoisted node reference
    pub hoisted: Option<Box<'a, JsChildNode<'a>>>,
    /// Identifiers declared in this expression
    pub identifiers: Option<Vec<'a, String>>,
    /// Whether this is a handler key
    pub is_handler_key: bool,
}

impl<'a> SimpleExpressionNode<'a> {
    pub fn new(content: impl Into<String>, is_static: bool, loc: SourceLocation) -> Self {
        Self {
            content: content.into(),
            is_static,
            const_type: if is_static {
                ConstantType::CanStringify
            } else {
                ConstantType::NotConstant
            },
            loc,
            js_ast: None,
            hoisted: None,
            identifiers: None,
            is_handler_key: false,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::SimpleExpression
    }
}

/// Placeholder for JavaScript expression AST from OXC
#[derive(Debug)]
pub struct JsExpression<'a> {
    /// Raw expression content (will be replaced with OXC AST)
    pub raw: String,
    _marker: std::marker::PhantomData<&'a ()>,
}

/// Compound expression node (mixed content)
#[derive(Debug)]
pub struct CompoundExpressionNode<'a> {
    pub children: Vec<'a, CompoundExpressionChild<'a>>,
    pub loc: SourceLocation,
    pub identifiers: Option<Vec<'a, String>>,
    pub is_handler_key: bool,
}

impl<'a> CompoundExpressionNode<'a> {
    pub fn new(allocator: &'a Bump, loc: SourceLocation) -> Self {
        Self {
            children: Vec::new_in(allocator),
            loc,
            identifiers: None,
            is_handler_key: false,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::CompoundExpression
    }
}

/// Child of a compound expression
#[derive(Debug)]
pub enum CompoundExpressionChild<'a> {
    Simple(Box<'a, SimpleExpressionNode<'a>>),
    Compound(Box<'a, CompoundExpressionNode<'a>>),
    Interpolation(Box<'a, InterpolationNode<'a>>),
    Text(Box<'a, TextNode>),
    String(String),
    Symbol(RuntimeHelper),
}

// ============================================================================
// Control Flow Nodes
// ============================================================================

/// If node (v-if)
#[derive(Debug)]
pub struct IfNode<'a> {
    pub branches: Vec<'a, IfBranchNode<'a>>,
    pub loc: SourceLocation,
    pub codegen_node: Option<IfCodegenNode<'a>>,
}

impl<'a> IfNode<'a> {
    pub fn new(allocator: &'a Bump, loc: SourceLocation) -> Self {
        Self {
            branches: Vec::new_in(allocator),
            loc,
            codegen_node: None,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::If
    }
}

/// If codegen node type
#[derive(Debug)]
pub enum IfCodegenNode<'a> {
    Conditional(Box<'a, ConditionalExpression<'a>>),
    Cache(Box<'a, CacheExpression<'a>>),
}

/// If branch node (v-if, v-else-if, v-else)
#[derive(Debug)]
pub struct IfBranchNode<'a> {
    pub condition: Option<ExpressionNode<'a>>,
    pub children: Vec<'a, TemplateChildNode<'a>>,
    pub user_key: Option<PropNode<'a>>,
    pub is_template_if: bool,
    pub loc: SourceLocation,
}

impl<'a> IfBranchNode<'a> {
    pub fn new(
        allocator: &'a Bump,
        condition: Option<ExpressionNode<'a>>,
        loc: SourceLocation,
    ) -> Self {
        Self {
            condition,
            children: Vec::new_in(allocator),
            user_key: None,
            is_template_if: false,
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::IfBranch
    }
}

/// For node (v-for)
#[derive(Debug)]
pub struct ForNode<'a> {
    pub source: ExpressionNode<'a>,
    pub value_alias: Option<ExpressionNode<'a>>,
    pub key_alias: Option<ExpressionNode<'a>>,
    pub object_index_alias: Option<ExpressionNode<'a>>,
    pub parse_result: ForParseResult<'a>,
    pub children: Vec<'a, TemplateChildNode<'a>>,
    pub loc: SourceLocation,
    pub codegen_node: Option<Box<'a, VNodeCall<'a>>>,
}

impl<'a> ForNode<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::For
    }
}

/// Parsed result for v-for expression
#[derive(Debug)]
pub struct ForParseResult<'a> {
    pub source: ExpressionNode<'a>,
    pub value: Option<ExpressionNode<'a>>,
    pub key: Option<ExpressionNode<'a>>,
    pub index: Option<ExpressionNode<'a>>,
    pub finalized: bool,
}

/// Text call node
#[derive(Debug)]
pub struct TextCallNode<'a> {
    pub content: TextCallContent<'a>,
    pub loc: SourceLocation,
    pub codegen_node: Option<TextCallCodegenNode<'a>>,
}

impl<'a> TextCallNode<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::TextCall
    }
}

/// Text call content
#[derive(Debug)]
pub enum TextCallContent<'a> {
    Text(Box<'a, TextNode>),
    Interpolation(Box<'a, InterpolationNode<'a>>),
    Compound(Box<'a, CompoundExpressionNode<'a>>),
}

/// Text call codegen node
#[derive(Debug)]
pub enum TextCallCodegenNode<'a> {
    Call(Box<'a, CallExpression<'a>>),
    Simple(Box<'a, SimpleExpressionNode<'a>>),
}

// ============================================================================
// Codegen Nodes
// ============================================================================

/// VNode call expression
#[derive(Debug)]
pub struct VNodeCall<'a> {
    pub tag: VNodeTag<'a>,
    pub props: Option<PropsExpression<'a>>,
    pub children: Option<VNodeChildren<'a>>,
    pub patch_flag: Option<PatchFlags>,
    pub dynamic_props: Option<DynamicProps<'a>>,
    pub directives: Option<DirectiveArguments<'a>>,
    pub is_block: bool,
    pub disable_tracking: bool,
    pub is_component: bool,
    pub loc: SourceLocation,
}

impl<'a> VNodeCall<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::VNodeCall
    }
}

/// VNode tag type
#[derive(Debug)]
pub enum VNodeTag<'a> {
    String(String),
    Symbol(RuntimeHelper),
    Call(Box<'a, CallExpression<'a>>),
}

/// VNode children type
#[derive(Debug)]
pub enum VNodeChildren<'a> {
    Multiple(Vec<'a, TemplateChildNode<'a>>),
    Single(TemplateTextChildNode<'a>),
    Slots(Box<'a, SlotsExpression<'a>>),
    ForRenderList(Box<'a, CallExpression<'a>>),
    Simple(Box<'a, SimpleExpressionNode<'a>>),
    Cache(Box<'a, CacheExpression<'a>>),
}

/// Template text child node
#[derive(Debug)]
pub enum TemplateTextChildNode<'a> {
    Text(Box<'a, TextNode>),
    Interpolation(Box<'a, InterpolationNode<'a>>),
    Compound(Box<'a, CompoundExpressionNode<'a>>),
}

/// Props expression type
#[derive(Debug)]
pub enum PropsExpression<'a> {
    Object(Box<'a, ObjectExpression<'a>>),
    Call(Box<'a, CallExpression<'a>>),
    Simple(Box<'a, SimpleExpressionNode<'a>>),
}

/// Dynamic props type
#[derive(Debug)]
pub enum DynamicProps<'a> {
    String(String),
    Simple(Box<'a, SimpleExpressionNode<'a>>),
}

/// Directive arguments
#[derive(Debug)]
pub struct DirectiveArguments<'a> {
    pub elements: Vec<'a, DirectiveArgumentNode<'a>>,
    pub loc: SourceLocation,
}

/// Single directive argument
#[derive(Debug)]
pub struct DirectiveArgumentNode<'a> {
    pub directive: String,
    pub exp: Option<ExpressionNode<'a>>,
    pub arg: Option<ExpressionNode<'a>>,
    pub modifiers: Option<Box<'a, ObjectExpression<'a>>>,
}

/// Slots expression
#[derive(Debug)]
pub enum SlotsExpression<'a> {
    Object(Box<'a, ObjectExpression<'a>>),
    Dynamic(Box<'a, CallExpression<'a>>),
}

// ============================================================================
// JavaScript AST Nodes
// ============================================================================

/// All JavaScript child node types for codegen
#[derive(Debug)]
pub enum JsChildNode<'a> {
    VNodeCall(Box<'a, VNodeCall<'a>>),
    Call(Box<'a, CallExpression<'a>>),
    Object(Box<'a, ObjectExpression<'a>>),
    Array(Box<'a, ArrayExpression<'a>>),
    Function(Box<'a, FunctionExpression<'a>>),
    Conditional(Box<'a, ConditionalExpression<'a>>),
    Cache(Box<'a, CacheExpression<'a>>),
    Assignment(Box<'a, AssignmentExpression<'a>>),
    Sequence(Box<'a, SequenceExpression<'a>>),
    SimpleExpression(Box<'a, SimpleExpressionNode<'a>>),
    CompoundExpression(Box<'a, CompoundExpressionNode<'a>>),
}

/// Codegen node union type
#[derive(Debug)]
pub enum CodegenNode<'a> {
    TemplateChild(TemplateChildNode<'a>),
    JsChild(JsChildNode<'a>),
    BlockStatement(Box<'a, BlockStatement<'a>>),
}

/// Call expression
#[derive(Debug)]
pub struct CallExpression<'a> {
    pub callee: Callee,
    pub arguments: Vec<'a, CallArgument<'a>>,
    pub loc: SourceLocation,
}

impl<'a> CallExpression<'a> {
    pub fn new(allocator: &'a Bump, callee: Callee, loc: SourceLocation) -> Self {
        Self {
            callee,
            arguments: Vec::new_in(allocator),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::JsCallExpression
    }
}

/// Callee type
#[derive(Debug)]
pub enum Callee {
    String(String),
    Symbol(RuntimeHelper),
}

/// Call argument type
#[derive(Debug)]
pub enum CallArgument<'a> {
    String(String),
    Symbol(RuntimeHelper),
    JsChild(JsChildNode<'a>),
    TemplateChild(TemplateChildNode<'a>),
    TemplateChildren(Vec<'a, TemplateChildNode<'a>>),
}

/// Object expression
#[derive(Debug)]
pub struct ObjectExpression<'a> {
    pub properties: Vec<'a, Property<'a>>,
    pub loc: SourceLocation,
}

impl<'a> ObjectExpression<'a> {
    pub fn new(allocator: &'a Bump, loc: SourceLocation) -> Self {
        Self {
            properties: Vec::new_in(allocator),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::JsObjectExpression
    }
}

/// Object property
#[derive(Debug)]
pub struct Property<'a> {
    pub key: ExpressionNode<'a>,
    pub value: JsChildNode<'a>,
    pub loc: SourceLocation,
}

impl<'a> Property<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsProperty
    }
}

/// Array expression
#[derive(Debug)]
pub struct ArrayExpression<'a> {
    pub elements: Vec<'a, ArrayElement<'a>>,
    pub loc: SourceLocation,
}

impl<'a> ArrayExpression<'a> {
    pub fn new(allocator: &'a Bump, loc: SourceLocation) -> Self {
        Self {
            elements: Vec::new_in(allocator),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::JsArrayExpression
    }
}

/// Array element type
#[derive(Debug)]
pub enum ArrayElement<'a> {
    String(String),
    Node(JsChildNode<'a>),
}

/// Function expression
#[derive(Debug)]
pub struct FunctionExpression<'a> {
    pub params: Option<FunctionParams<'a>>,
    pub returns: Option<FunctionReturns<'a>>,
    pub body: Option<FunctionBody<'a>>,
    pub newline: bool,
    pub is_slot: bool,
    pub is_non_scoped_slot: bool,
    pub loc: SourceLocation,
}

impl<'a> FunctionExpression<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsFunctionExpression
    }
}

/// Function parameters
#[derive(Debug)]
pub enum FunctionParams<'a> {
    Single(ExpressionNode<'a>),
    String(String),
    Multiple(Vec<'a, FunctionParam<'a>>),
}

/// Single function parameter
#[derive(Debug)]
pub enum FunctionParam<'a> {
    Expression(ExpressionNode<'a>),
    String(String),
}

/// Function returns
#[derive(Debug)]
pub enum FunctionReturns<'a> {
    Single(TemplateChildNode<'a>),
    Multiple(Vec<'a, TemplateChildNode<'a>>),
    JsChild(JsChildNode<'a>),
}

/// Function body
#[derive(Debug)]
pub enum FunctionBody<'a> {
    Block(Box<'a, BlockStatement<'a>>),
    If(Box<'a, IfStatement<'a>>),
}

/// Conditional expression (ternary)
#[derive(Debug)]
pub struct ConditionalExpression<'a> {
    pub test: JsChildNode<'a>,
    pub consequent: JsChildNode<'a>,
    pub alternate: JsChildNode<'a>,
    pub newline: bool,
    pub loc: SourceLocation,
}

impl<'a> ConditionalExpression<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsConditionalExpression
    }
}

/// Cache expression
#[derive(Debug)]
pub struct CacheExpression<'a> {
    pub index: u32,
    pub value: JsChildNode<'a>,
    pub need_pause_tracking: bool,
    pub in_v_once: bool,
    pub need_array_spread: bool,
    pub loc: SourceLocation,
}

impl<'a> CacheExpression<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsCacheExpression
    }
}

// ============================================================================
// SSR Codegen Nodes
// ============================================================================

/// Block statement
#[derive(Debug)]
pub struct BlockStatement<'a> {
    pub body: Vec<'a, BlockStatementBody<'a>>,
    pub loc: SourceLocation,
}

impl<'a> BlockStatement<'a> {
    pub fn new(allocator: &'a Bump, loc: SourceLocation) -> Self {
        Self {
            body: Vec::new_in(allocator),
            loc,
        }
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::JsBlockStatement
    }
}

/// Block statement body item
#[derive(Debug)]
pub enum BlockStatementBody<'a> {
    JsChild(JsChildNode<'a>),
    If(Box<'a, IfStatement<'a>>),
}

/// Template literal
#[derive(Debug)]
pub struct TemplateLiteral<'a> {
    pub elements: Vec<'a, TemplateLiteralElement<'a>>,
    pub loc: SourceLocation,
}

impl<'a> TemplateLiteral<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsTemplateLiteral
    }
}

/// Template literal element
#[derive(Debug)]
pub enum TemplateLiteralElement<'a> {
    String(String),
    JsChild(JsChildNode<'a>),
}

/// If statement (SSR)
#[derive(Debug)]
pub struct IfStatement<'a> {
    pub test: ExpressionNode<'a>,
    pub consequent: Box<'a, BlockStatement<'a>>,
    pub alternate: Option<IfStatementAlternate<'a>>,
    pub loc: SourceLocation,
}

impl<'a> IfStatement<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsIfStatement
    }
}

/// If statement alternate
#[derive(Debug)]
pub enum IfStatementAlternate<'a> {
    If(Box<'a, IfStatement<'a>>),
    Block(Box<'a, BlockStatement<'a>>),
    Return(Box<'a, ReturnStatement<'a>>),
}

/// Assignment expression
#[derive(Debug)]
pub struct AssignmentExpression<'a> {
    pub left: Box<'a, SimpleExpressionNode<'a>>,
    pub right: JsChildNode<'a>,
    pub loc: SourceLocation,
}

impl<'a> AssignmentExpression<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsAssignmentExpression
    }
}

/// Sequence expression
#[derive(Debug)]
pub struct SequenceExpression<'a> {
    pub expressions: Vec<'a, JsChildNode<'a>>,
    pub loc: SourceLocation,
}

impl<'a> SequenceExpression<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsSequenceExpression
    }
}

/// Return statement
#[derive(Debug)]
pub struct ReturnStatement<'a> {
    pub returns: ReturnValue<'a>,
    pub loc: SourceLocation,
}

impl<'a> ReturnStatement<'a> {
    pub fn node_type(&self) -> NodeType {
        NodeType::JsReturnStatement
    }
}

/// Return value type
#[derive(Debug)]
pub enum ReturnValue<'a> {
    Single(TemplateChildNode<'a>),
    Multiple(Vec<'a, TemplateChildNode<'a>>),
    JsChild(JsChildNode<'a>),
}
