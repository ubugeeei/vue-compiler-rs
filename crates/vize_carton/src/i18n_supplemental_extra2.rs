//! Additional supplemental i18n entries split out to keep source files small.

use rustc_hash::FxHashMap;

type MessageMap = FxHashMap<&'static str, &'static str>;

/// Insert every extra supplemental entry into the locale message maps.
pub(crate) fn register(messages: &mut [MessageMap; 3]) {
    for &(key, en, ja, zh) in ENTRIES {
        messages[0].insert(key, en);
        messages[1].insert(key, ja);
        messages[2].insert(key, zh);
    }
}

/// Extra supplemental translation entries: `(key, en, ja, zh)`.
static ENTRIES: &[(&str, &str, &str, &str)] = &[
    // vue/no-multiple-template-root
    (
        "vue/no-multiple-template-root.multiple_root",
        "The template root requires exactly one element.",
        "テンプレートのルートには要素が1つだけ必要です。",
        "模板根节点必须恰好包含一个元素。",
    ),
    (
        "vue/no-multiple-template-root.text_root",
        "The template root requires an element rather than texts.",
        "テンプレートのルートにはテキストではなく要素が必要です。",
        "模板根节点需要元素而不是文本。",
    ),
    (
        "vue/no-multiple-template-root.disallowed_element",
        "The template root disallows '<{tag}>' elements.",
        "テンプレートのルートでは '<{tag}>' 要素を使用できません。",
        "模板根节点不允许 '<{tag}>' 元素。",
    ),
    (
        "vue/no-multiple-template-root.disallowed_directive",
        "The template root disallows 'v-for' directives.",
        "テンプレートのルートでは 'v-for' ディレクティブを使用できません。",
        "模板根节点不允许 'v-for' 指令。",
    ),
    (
        "vue/no-multiple-template-root.help",
        "Wrap the template in one rendered element; keep conditional branches in one v-if chain.",
        "テンプレートを1つの描画要素で囲み、条件分岐は1つのv-ifチェーンにまとめてください。",
        "请用一个渲染元素包裹模板，并将条件分支保留在同一个v-if链中。",
    ),
    // vue/no-invalid-html-attribute
    (
        "vue/no-invalid-html-attribute.description",
        "Disallow invalid static values for HTML attributes",
        "HTML属性の無効な静的値を禁止する",
        "禁止HTML属性使用无效的静态值",
    ),
    (
        "vue/no-invalid-html-attribute.empty",
        "The `rel` attribute must not be empty",
        "`rel`属性を空にしてはいけません",
        "`rel`属性不能为空",
    ),
    (
        "vue/no-invalid-html-attribute.wrong_tag",
        "The `rel` attribute is not valid on `<{tag}>`",
        "`rel`属性は`<{tag}>`では有効ではありません",
        "`rel`属性在`<{tag}>`上无效",
    ),
    (
        "vue/no-invalid-html-attribute.invalid",
        "`{value}` is not a valid `rel` value",
        "`{value}`は有効な`rel`値ではありません",
        "`{value}`不是有效的`rel`值",
    ),
    (
        "vue/no-invalid-html-attribute.invalid_for_tag",
        "`{value}` is not a valid `rel` value on `<{tag}>`",
        "`{value}`は`<{tag}>`で有効な`rel`値ではありません",
        "`{value}`不是`<{tag}>`上的有效`rel`值",
    ),
    (
        "vue/no-invalid-html-attribute.shortcut",
        "`shortcut` in `rel` must be followed by `icon`",
        "`rel`内の`shortcut`は`icon`の直前に置く必要があります",
        "`rel`中的`shortcut`后面必须跟随`icon`",
    ),
    (
        "vue/no-invalid-html-attribute.help",
        "Use only standard `rel` tokens that are allowed for this element, such as `noopener noreferrer` on links or `stylesheet` on link elements.",
        "この要素で許可されている標準の`rel`トークンだけを使ってください。例: リンクの`noopener noreferrer`、link要素の`stylesheet`。",
        "请只使用此元素允许的标准`rel`标记，例如链接上的`noopener noreferrer`或link元素上的`stylesheet`。",
    ),
    // vue/v-on-event-hyphenation
    (
        "vue/v-on-event-hyphenation.message_never",
        "Custom event listeners on components should not be hyphenated: '{name}'",
        "コンポーネントのカスタムイベントリスナーはハイフン区切りにしないでください: '{name}'",
        "组件上的自定义事件监听器不应使用连字符：'{name}'",
    ),
    (
        "vue/v-on-event-hyphenation.help_never",
        "Use camelCase or a single-word event name instead of kebab-case.",
        "ケバブケースではなく、camelCase または単一語のイベント名を使用してください。",
        "请使用 camelCase 或单词事件名，而不是 kebab-case。",
    ),
    // vue/attribute-hyphenation
    (
        "vue/attribute-hyphenation.message_never",
        "Attribute should not be hyphenated",
        "属性はハイフン区切りにしないでください",
        "属性不应使用连字符命名",
    ),
    (
        "vue/attribute-hyphenation.help_never",
        "Use camelCase or a single-word attribute name instead of kebab-case.",
        "ケバブケースではなく、camelCase または単一語の属性名を使用してください。",
        "请使用 camelCase 或单词属性名，而不是 kebab-case。",
    ),
];
