---
title: Vue ルール
---

<!-- Generated translation; source: rules/vue.md -->

# Vue ルール

Vue ルールは、Patina の単一ファイル ルールです。 SFC テンプレートの構造、ディレクティブの構文、
コードがランタイムに到達する前に、コンポーネントの名前付け、および Vue 固有の正確さの危険が発生します。

## `vue/require-v-for-key`

すべての `v-for` ノードに安定したキーが必要です。

デフォルトの重大度: `error`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
```

良い：

```vue
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
```

## `vue/no-use-v-if-with-v-for`

`v-if` と `v-for` を同時に持つノードをレポートします。計算された値をフィルタリングすると、
リストのアイデンティティが安定し、テンプレートの分析が容易になります。

デフォルトの重大度: `warning`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <li v-for="item in items" v-if="item.visible" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

良い：

```vue
<script setup lang="ts">
const visibleItems = computed(() => items.filter((item) => item.visible));
</script>

<template>
  <li v-for="item in visibleItems" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

## `vue/no-mutating-props`

レポートは props に書き込みます。所有コンポーネントは、イベントまたはモデルを通じて値を更新する必要があります。
バインディング。

デフォルトの重大度: `error`
プリセット: `happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();

props.count++;
</script>
```

良い：

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();
const emit = defineEmits<{ "update:count": [value: number] }>();

function increment() {
  emit("update:count", props.count + 1);
}
</script>
```

## `vue/no-v-html`

生の HTML をレンダリングし、ユーザー制御のコンテンツを XSS シンクに変換できるため、`v-html` をレポートします。

デフォルトの重大度: `warning`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <article v-html="content" />
</template>
```

良い：

```vue
<template>
  <article>{{ content }}</article>
</template>
```

## `vue/no-child-content`

`v-html` または `v-text` も使用する要素の子コンテンツをレポートします。 Vue は次の子を置き換えます。
そのため、作成されたコンテンツは誤解を招く可能性があります。

デフォルトの重大度: `error`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <p v-text="message">Fallback text</p>
</template>
```

良い：

```vue
<template>
  <p v-text="message" />
</template>
```

## `vue/no-duplicate-attributes`

同じ要素の重複した属性を報告します。

デフォルトの重大度: `error`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <button class="primary" class="large">Save</button>
</template>
```

良い：

```vue
<template>
  <button class="primary large">Save</button>
</template>
```

## `vue/no-dupe-v-else-if`

`v-if` / `v-else-if` チェーン内の繰り返し条件をレポートします。

デフォルトの重大度: `error`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'ready'">Still ready</p>
</template>
```

良い：

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'loading'">Loading</p>
</template>
```

## `vue/no-template-shadow`

外部スコープの変数をシャドウするテンプレート変数をレポートします。これにより偶発的な事故が防止されます
読者が期待するものとは異なる値への参照。

デフォルトの重大度: `warning`
プリセット: `nuxt`、`opinionated`

悪い：

```vue
<script setup lang="ts">
const item = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

良い：

```vue
<script setup lang="ts">
const selectedItem = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

## `vue/no-unsafe-url`

次のような安全でないスキームに解決される可能性がある URL バインディングと静的 URL 属性をレポートします。
`javascript:`、`vbscript:`、または実行可能な `data:` ペイロード。

デフォルトの重大度: `warning`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <iframe src="javascript:alert(1)"></iframe>
  <object data="data:text/html,<script>alert(1)</script>"></object>
  <img srcset="/safe.png 1x, javascript:alert(1) 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

良い：

```vue
<script setup lang="ts">
const rawNextUrl = ref("/next");
const nextUrl = computed(() => {
  return rawNextUrl.value.startsWith("/") ? rawNextUrl.value : "/";
});
</script>

<template>
  <iframe src="/embedded/report" title="Report"></iframe>
  <img srcset="/avatar.png 1x, /avatar@2x.png 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

## `vue/no-unused-components`

テンプレートには決して表示されない、ローカルに登録されたコンポーネントをレポートします。

デフォルトの重大度: `warning`
プリセット: `happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <p>{{ user.name }}</p>
</template>
```

良い：

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <UserAvatar :user="user" />
</template>
```

## `vue/no-unused-properties`

コンポーネントによって使用されていない、`defineProps` を通じて宣言された props を報告します。

デフォルトの重大度: `warning`
プリセット: `happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
</template>
```

良い：

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
  <p>{{ description }}</p>
</template>
```

## `vue/require-component-is`

`is` バインディングなしで `<component>` をレポートします。

デフォルトの重大度: `error`
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`

悪い：

```vue
<template>
  <component />
</template>
```

良い：

```vue
<template>
  <component :is="currentComponent" />
</template>
```

## `vue/use-unique-element-ids`

コンポーネントの再利用と SSR にとって `useId()` がより安全な場所の静的リテラル ID をレポートします。

デフォルトの重大度: `warning`
プリセット: `nuxt`、`opinionated`

悪い：

```vue
<template>
  <label for="email">Email</label>
  <input id="email" />
</template>
```

良い：

```vue
<script setup lang="ts">
const emailId = useId();
</script>

<template>
  <label :for="emailId">Email</label>
  <input :id="emailId" />
</template>
```

## 構文とスタイルの規則

これらのルールには長い例は必要ありませんが、依然として第一級のルールとして動作し、
名前で設定されます。

`vue/attribute-hyphenation` は、カスタム コンポーネントに属性命名スタイルを適用します。デフォルト:
`warning`。プリセット: `happy-path`、`nuxt`、`opinionated`。

`vue/attribute-order` は、安定した属性順序を強制します。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/component-definition-name-casing` は、PascalCase コンポーネント定義名を強制します。デフォルト:
`warning`。プリセット: `happy-path`、`nuxt`、`opinionated`。

`vue/component-name-in-template-casing` は、テンプレート内のコンポーネント名の大文字と小文字を強制します。デフォルト:
`warning`。プリセット: `nuxt`、`opinionated`。

`vue/html-quotes` は、HTML 属性の引用スタイルを強制します。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/html-self-closing` は自動終了スタイルを強制します。デフォルト: `warning`。プリセット: `nuxt`、
`opinionated`。`linter.ruleOptions["vue/html-self-closing"]` で `html.void`、`html.normal`、
`html.component`、`svg`、`math` を設定できます。各値は `"always"`、`"never"`、`"any"` です。
Vize のデフォルトは void HTML、コンポーネント、SVG、MathML が `"always"`、通常 HTML が `"any"` です。

`vue/multi-word-component-names` では、コンポーネント名に複数の単語が含まれる必要があります。デフォルト:
`error`。プリセット: `essential`、`nuxt`、`opinionated`。

`vue/mustache-interpolation-spacing` は、口ひげ補間内のスペースを強制します。デフォルト:
`warning`。プリセット: `happy-path`、`nuxt`、`opinionated`。

`vue/no-boolean-attr-value` は、ブール型 HTML 属性の明示的な値を許可しません。デフォルト:
`warning`。プリセット: `nuxt`、`opinionated`。

`vue/no-inline-style` は、インライン `style` 属性を推奨しません。デフォルト: `warning`。プリセット: `nuxt`、
`opinionated`。

`vue/no-lone-template` は、不要な `<template>` ラッパーを禁止します。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/no-multi-spaces` では、テンプレート内でスペースを繰り返すことは許可されません。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/no-non-component-keep-alive-child` は、`<KeepAlive>` 直下のプレーン要素ラッパーを報告します。
Vue がキャッシュできるのはコンポーネント VNode だけだからです。デフォルト: `warning`。プリセット: なし
（オプトイン）。`v-show` だけのラッパーは無視します。

`vue/no-preprocessor-lang` は、SFC ブロック内の CSS プリプロセッサ言語を抑制します。デフォルト: `warning`。
プリセット: `nuxt`、`opinionated`。

`vue/no-reserved-component-names` では、予約された HTML 名または Vue 名をコンポーネント名として使用できません。デフォルト:
`error`。プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-script-non-standard-lang` は、非標準のスクリプト言語を抑制します。デフォルト: `warning`。
プリセット: `nuxt`、`opinionated`。

`vue/no-src-attribute` は、SFC ブロックの外部 `src` 属性を抑制します。デフォルト: `warning`。
プリセット: `nuxt`、`opinionated`。

`vue/no-template-key` は、`<template>` での `key` を禁止します。デフォルト: `error`。プリセット: `essential`、
`happy-path`、`nuxt`、`opinionated`。

`vue/no-template-lang` は、`<template>` での `lang` を阻止します。デフォルト: `warning`。プリセット: `nuxt`、
`opinionated`。

`vue/no-textarea-mustache` は、`<textarea>` 内での口ひげ補間を禁止します。デフォルト: `error`。
プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-unused-vars` は、`v-for` および `v-slot` によって導入された未使用の変数をレポートします。デフォルト:
`warning`。プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-useless-template-attributes` は、Vue が無視する `<template>` の属性を禁止します。デフォルト:
`error`。プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-v-text-v-html-on-component` は、コンポーネント要素で `v-text` または `v-html` を禁止します。デフォルト:
`error`。プリセット: `essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/permitted-contents` は、Vue テンプレート内で HTML コンテンツ モデル ルールを適用します。デフォルト: `error`。
プリセット: `happy-path`、`nuxt`、`opinionated`。

`vue/prefer-props-shorthand` は、小道具の省略構文を推奨します。デフォルト: `warning`。プリセット:
`nuxt`、`opinionated`。

`vue/prop-name-casing` は、`defineProps` で宣言された prop 名のケーシング（既定は `camelCase`）を強制
します。テンプレート側は `vue/attribute-hyphenation` が担当します。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/require-component-registration` では、明示的なコンポーネントのインポートまたは登録が必要です。デフォルト:
`warning`。プリセット: `opinionated`。

`vue/require-scoped-style` には、SFC スタイル ブロックで `scoped` が必要です。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/scoped-event-names` では、`form:submit` などのスコープ付きイベント名を推奨します。デフォルト: `warning`。
プリセット: `nuxt`、`opinionated`。

`vue/sfc-element-order` は、トップレベルの SFC ブロックの順序を強制します。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

`vue/single-style-block` では、スタイルを 1 つのブロックにまとめることをお勧めします。デフォルト: `warning`。プリセット:
`happy-path`、`nuxt`、`opinionated`。

修飾子ベースのハンドラーが共存する場合、`vue/use-v-on-exact` は `.exact` を強制します。デフォルト: `warning`。
プリセット: `essential`、`nuxt`、`opinionated`。

`vue/v-bind-style`、`vue/v-on-style`、および `vue/v-slot-style` は、ディレクティブのスタイル設定を強制します。
デフォルト: `warning`。プリセット: `nuxt` および/または `happy-path`、および `opinionated`。

`vue/valid-attribute-name`、`vue/valid-v-bind`、`vue/valid-v-else`、`vue/valid-v-for`、
`vue/valid-v-if`、`vue/valid-v-memo`、`vue/valid-v-model`、`vue/valid-v-on`、`vue/valid-v-show`、
および `vue/valid-v-slot` は、無効な Vue ディレクティブ構文を報告します。デフォルト: `error`。プリセット:
`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/warn-custom-block` および `vue/warn-custom-directive` は、カスタム Vue 拡張ポイントについて警告します。
ホストのサポートまたは登録が必要です。デフォルト: `warning`。プリセット: `nuxt`、`opinionated`。
