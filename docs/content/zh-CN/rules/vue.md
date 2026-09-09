---
title: Vue规则
---

<!-- Generated translation; source: rules/vue.md -->

# Vue 规则

Vue规则是Patina单列规则。他们检查SFC模板结构、指令语法，
组件命名，以及代码到达运行时之前的Vue特定正确性风险。

## `vue/require-v-for-key`

要求每个`v-for`节点都必须有一个稳定的密钥。

默认严重程度：`error`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
```

好：

```vue
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
```

## `vue/no-use-v-if-with-v-for`

报告节点同时拥有`v-if`和`v-for`。在计算值中进行过滤时，会保持
列表身份稳定，使模板更容易分析。

默认严重程度：`warning`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <li v-for="item in items" v-if="item.visible" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

好：

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

报告写信给道具。拥有组件应通过事件或模型更新该值
束缚。

默认严重程度：`error`
预设：`happy-path`，`nuxt`，`opinionated`

缺点：

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();

props.count++;
</script>
```

好：

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

报告`v-html`因为它渲染原始 HTML，并能将用户控制的内容转化为 XSS 汇入。

默认严重程度：`warning`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <article v-html="content" />
</template>
```

好：

```vue
<template>
  <article>{{ content }}</article>
</template>
```

## `vue/no-child-content`

报告使用`v-html`或`v-text`元素的儿童内容。Vue 取代了
因此，作者内容具有误导性。

默认严重程度：`error`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <p v-text="message">Fallback text</p>
</template>
```

好：

```vue
<template>
  <p v-text="message" />
</template>
```

## `vue/no-duplicate-attributes`

报告在同一元素上重复属性。

默认严重程度：`error`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <button class="primary" class="large">Save</button>
</template>
```

好：

```vue
<template>
  <button class="primary large">Save</button>
</template>
```

## `vue/no-dupe-v-else-if`

报告在`v-if`/`v-else-if`链中反复出现病症。

默认严重程度：`error`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'ready'">Still ready</p>
</template>
```

好：

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'loading'">Loading</p>
</template>
```

## `vue/no-template-shadow`

报告模板变量，从外部范围中遮挡变量。这样可以防止意外发生
引用的数值与读者预期不同。

默认严重程度：`warning`
预设：`nuxt`，`opinionated`

缺点：

```vue
<script setup lang="ts">
const item = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

好：

```vue
<script setup lang="ts">
const selectedItem = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

## `vue/no-unsafe-url`

报告可能解析为不安全方案的URL绑定和静态URL属性，如
`javascript:`、`vbscript:`或可执行`data:`载荷。

默认严重程度：`warning`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <iframe src="javascript:alert(1)"></iframe>
  <object data="data:text/html,<script>alert(1)</script>"></object>
  <img srcset="/safe.png 1x, javascript:alert(1) 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

好：

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

报告本地注册的组件，但这些组件从未出现在模板中。

默认严重程度：`warning`
预设：`happy-path`，`nuxt`，`opinionated`

缺点：

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <p>{{ user.name }}</p>
</template>
```

好：

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <UserAvatar :user="user" />
</template>
```

## `vue/no-unused-properties`

报告通过`defineProps`声明但组件未使用的道具。

默认严重程度：`warning`
预设：`happy-path`，`nuxt`，`opinionated`

缺点：

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
</template>
```

好：

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

报告`<component>`没有 `is` 约束。

默认严重程度：`error`
预设：`essential`、`happy-path`、`nuxt`、`opinionated`

缺点：

```vue
<template>
  <component />
</template>
```

好：

```vue
<template>
  <component :is="currentComponent" />
</template>
```

## `vue/use-unique-element-ids`

报告静态的文字ID，位于`useId()`更安全的组件重用和SSR位置。

默认严重程度：`warning`
预设：`nuxt`，`opinionated`

缺点：

```vue
<template>
  <label for="email">Email</label>
  <input id="email" />
</template>
```

好：

```vue
<script setup lang="ts">
const emailId = useId();
</script>

<template>
  <label :for="emailId">Email</label>
  <input :id="emailId" />
</template>
```

## 句法与风格规则

这些规则不需要长例说明，但它们仍然表现为一类规则，并且可以
按名称配置。

`vue/attribute-hyphenation`会对自定义组件强制属性命名风格。默认：
`warning`。预设：`happy-path`、`nuxt`、`opinionated`。

`vue/attribute-order`强制执行稳定的属性顺序。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/component-definition-name-casing`强制执行PascalCase组件定义名称。默认：
`warning`。预设：`happy-path`、`nuxt`、`opinionated`。

`vue/component-name-in-template-casing` 在模板中强制组件命名外壳。默认：
`warning`。预设：`nuxt`，`opinionated`。

`vue/html-quotes` 对 HTML 属性强制使用引号样式。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/html-self-closing`强制执行自我闭合风格。默认值：`warning`。预设：`nuxt`，
`opinionated`。可通过 `linter.ruleOptions["vue/html-self-closing"]` 配置 `html.void`、
`html.normal`、`html.component`、`svg` 和 `math`。每个值都接受 `"always"`、`"never"` 或 `"any"`。
Vize 默认对 void HTML、组件、SVG 和 MathML 使用 `"always"`，对普通 HTML 使用 `"any"`。

`vue/multi-word-component-names`要求组件名称包含多个词。默认：
`error`。预设：`essential`、`nuxt`、`opinionated`。

`vue/mustache-interpolation-spacing`在胡须插值内强制间距。默认：
`warning`。预设：`happy-path`、`nuxt`、`opinionated`。

`vue/no-boolean-attr-value`不允许对布尔 HTML 属性提供显式值。默认：
`warning`。预设：`nuxt`，`opinionated`。

`vue/no-inline-style`不鼓励内联的`style`属性。默认：`warning`。预设：`nuxt`，
`opinionated`。

`vue/no-lone-template`禁止不必要的`<template>`包装。默认：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/no-multi-spaces`不允许模板中重复空格。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/no-non-component-keep-alive-child` 会报告 `<KeepAlive>` 直下的普通元素包装器，因为 Vue
只能缓存组件 VNode。默认值：`warning`。预设：无（选择加入）。只使用 `v-show` 的包装器会被忽略。

`vue/no-preprocessor-lang`不鼓励在SFC块中使用CSS预处理器语言。默认值：`warning`。
预设：`nuxt`，`opinionated`。

`vue/no-reserved-component-names`不允许保留HTML或Vue名称作为组件名称。默认：
`error`。预设：`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-script-non-standard-lang`不鼓励使用非标准文字语言。默认值：`warning`。
预设：`nuxt`，`opinionated`。

`vue/no-src-attribute`不鼓励在SFC块上使用外部`src`属性。默认：`warning`。
预设：`nuxt`，`opinionated`。

`vue/no-template-key`禁止`<template>`上进行`key`。默认值：`error`。预设：`essential`，
`happy-path`，`nuxt`，`opinionated`。

`vue/no-template-lang`不鼓励`lang`在`<template>`上。默认值：`warning`。预设：`nuxt`，
`opinionated`。

`vue/no-textarea-mustache`禁止在`<textarea>`内插入胡须。默认：`error`。
预设：`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-unused-vars`报告由`v-for`和`v-slot`引入的未使用的变量。默认：
`warning`。预设：`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-useless-template-attributes` 不允许 Vue 忽略的属性在 `<template>` 上。默认：
`error`。预设：`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/no-v-text-v-html-on-component`不允许对组件进行 `v-text` 或 `v-html`。默认：
`error`。预设：`essential`、`happy-path`、`nuxt`、`opinionated`。

`vue/permitted-contents`在Vue模板中强制执行HTML内容模型规则。默认值：`error`。
预设：`happy-path`、`nuxt`、`opinionated`。

`vue/prefer-props-shorthand`推荐道具用速记语法。默认值：`warning`。预设：
`nuxt`，好，`opinionated`。

`vue/prop-name-casing` 强制 `defineProps` 声明的 prop 名使用指定的命名风格（默认 `camelCase`）；模板一
侧由 `vue/attribute-hyphenation` 负责。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/require-component-registration`需要明确导入或注册组件。默认：
`warning`。预设：`opinionated`。

`vue/require-scoped-style`需要`scoped` SFC风格的方块。默认：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/scoped-event-names`推荐使用如`form:submit`这样的范围事件名称。默认：`warning`。
预设：`nuxt`，`opinionated`。

`vue/sfc-element-order`强制执行顶层SFC块的顺序。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/single-style-block`建议把风格放在一个区块里。默认值：`warning`。预设：
`happy-path`，`nuxt`，`opinionated`。

`vue/use-v-on-exact`在基于修饰符的处理器共存时强制执行`.exact`。默认：`warning`。
预设：`essential`、`nuxt`、`opinionated`。

`vue/v-bind-style`、`vue/v-on-style`和`vue/v-slot-style`强制执行指令式样式偏好。
默认值：`warning`。预设：`nuxt`和/或`happy-path`，加上`opinionated`。

`vue/valid-attribute-name`，`vue/valid-v-bind`，`vue/valid-v-else`，`vue/valid-v-for`，
`vue/valid-v-if`，`vue/valid-v-memo`，`vue/valid-v-model`，`vue/valid-v-on`，`vue/valid-v-show`，
并`vue/valid-v-slot`报告无效的Vue指令语法。默认：`error`。预设：
`essential`，`happy-path`，`nuxt`，`opinionated`。

`vue/warn-custom-block`和`vue/warn-custom-directive`警告关于自定义Vue扩展点的建议
需要主机支持或注册。默认：`warning`。预设：`nuxt`，`opinionated`。
