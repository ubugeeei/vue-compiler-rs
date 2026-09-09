/// Options for `vue/no-mutating-props`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoMutatingPropsOptions {
    /// Allow mutating nested properties of props while still rejecting direct
    /// prop replacement.
    pub shallow_only: bool,
}
