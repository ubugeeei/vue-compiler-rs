---
title: Vue Rules
---

# Vue Rules

Vue rules are Patina single-file rules. They inspect SFC template structure, directive syntax,
component naming, and Vue-specific correctness hazards before the code reaches the runtime.

## `vue/require-v-for-key`

Requires every `v-for` node to have a stable key.

Default severity: `error`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
```

Good:

```vue
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
```

## `vue/no-use-v-if-with-v-for`

Reports a node that has `v-if` and `v-for` at the same time. Filtering in a computed value keeps the
list identity stable and makes the template easier to analyze.

Default severity: `warning`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <li v-for="item in items" v-if="item.visible" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

Good:

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

Reports writes to props. The owning component should update the value through an event or a model
binding.

Default severity: `error`  
Presets: `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();

props.count++;
</script>
```

Good:

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

Reports `v-html` because it renders raw HTML and can turn user-controlled content into an XSS sink.

Default severity: `warning`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <article v-html="content" />
</template>
```

Good:

```vue
<template>
  <article>{{ content }}</article>
</template>
```

## `vue/no-child-content`

Reports child content on elements that also use `v-html` or `v-text`. Vue replaces the children at
runtime, so the authored content is misleading.

Default severity: `error`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <p v-text="message">Fallback text</p>
</template>
```

Good:

```vue
<template>
  <p v-text="message" />
</template>
```

## `vue/no-duplicate-attributes`

Reports duplicate attributes on the same element.

Default severity: `error`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <button class="primary" class="large">Save</button>
</template>
```

Good:

```vue
<template>
  <button class="primary large">Save</button>
</template>
```

## `vue/no-dupe-v-else-if`

Reports repeated conditions in a `v-if` / `v-else-if` chain.

Default severity: `error`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'ready'">Still ready</p>
</template>
```

Good:

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'loading'">Loading</p>
</template>
```

## `vue/no-template-shadow`

Reports template variables that shadow variables from an outer scope. This prevents accidental
references to a different value than the reader expects.

Default severity: `warning`  
Presets: `nuxt`, `opinionated`

Bad:

```vue
<script setup lang="ts">
const item = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

Good:

```vue
<script setup lang="ts">
const selectedItem = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

## `vue/no-unsafe-url`

Reports URL bindings and static URL attributes that may resolve to unsafe schemes such as
`javascript:`, `vbscript:`, or executable `data:` payloads.

Default severity: `warning`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <iframe src="javascript:alert(1)"></iframe>
  <object data="data:text/html,<script>alert(1)</script>"></object>
  <img srcset="/safe.png 1x, javascript:alert(1) 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

Good:

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

Reports locally registered components that never appear in the template.

Default severity: `warning`  
Presets: `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <p>{{ user.name }}</p>
</template>
```

Good:

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <UserAvatar :user="user" />
</template>
```

## `vue/no-unused-properties`

Reports props declared through `defineProps` that are not used by the component.

Default severity: `warning`  
Presets: `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
</template>
```

Good:

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

Reports `<component>` without an `is` binding.

Default severity: `error`  
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Bad:

```vue
<template>
  <component />
</template>
```

Good:

```vue
<template>
  <component :is="currentComponent" />
</template>
```

## `vue/use-unique-element-ids`

Reports static literal IDs in places where `useId()` is safer for component reuse and SSR.

Default severity: `warning`  
Presets: `nuxt`, `opinionated`

Bad:

```vue
<template>
  <label for="email">Email</label>
  <input id="email" />
</template>
```

Good:

```vue
<script setup lang="ts">
const emailId = useId();
</script>

<template>
  <label :for="emailId">Email</label>
  <input :id="emailId" />
</template>
```

## Syntax And Style Rules

These rules do not need long examples, but they still behave as first-class rules.

`vue/attribute-hyphenation` enforces attribute naming style on custom components. Default:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/attribute-order` enforces a stable attribute order. Default: `warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/component-definition-name-casing` enforces PascalCase component definition names. Default:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/component-name-in-template-casing` enforces component name casing in templates. Default:
`warning`. Presets: `nuxt`, `opinionated`.

`vue/html-quotes` enforces quote style for HTML attributes. Default: `warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/html-self-closing` enforces self-closing style. Default: `warning`. Presets: `nuxt`, `opinionated`.
`linter.ruleOptions["vue/html-self-closing"]` accepts `html.void`, `html.normal`, `html.component`, `svg`, and `math`; each is `"always"`, `"never"`, or `"any"`.

`vue/multi-word-component-names` requires component names to contain more than one word. Default: `error`. Presets: `essential`, `nuxt`, `opinionated`.

`vue/mustache-interpolation-spacing` enforces spacing inside mustache interpolation. Default:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/no-boolean-attr-value` disallows explicit values for boolean HTML attributes. Default:
`warning`. Presets: `nuxt`, `opinionated`.

`vue/no-inline-style` discourages inline `style` attributes. Default: `warning`. Presets: `nuxt`,
`opinionated`.

`vue/no-lone-template` disallows unnecessary `<template>` wrappers. Default: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/no-multi-spaces` disallows repeated spaces in templates. Default: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/no-non-component-keep-alive-child` reports plain element wrappers directly below `<KeepAlive>` because Vue only caches component VNodes. Default: `warning`. Presets: none (opt-in).
`v-show`-only wrappers are ignored.

`vue/no-preprocessor-lang` discourages CSS preprocessor languages in SFC blocks. Default: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-reserved-component-names` disallows reserved HTML or Vue names as component names. Default:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-script-non-standard-lang` discourages non-standard script languages. Default: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-src-attribute` discourages external `src` attributes on SFC blocks. Default: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-template-key` disallows `key` on `<template>`. Default: `error`. Presets: `essential`,
`happy-path`, `nuxt`, `opinionated`.

`vue/no-template-lang` discourages `lang` on `<template>`. Default: `warning`. Presets: `nuxt`,
`opinionated`.

`vue/no-textarea-mustache` disallows mustache interpolation inside `<textarea>`. Default: `error`.
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-unused-vars` reports unused variables introduced by `v-for` and `v-slot`. Default:
`warning`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-useless-template-attributes` disallows attributes on `<template>` that Vue ignores. Default:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-v-text-v-html-on-component` disallows `v-text` or `v-html` on component elements. Default:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/permitted-contents` enforces HTML content model rules inside Vue templates. Default: `error`.
Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/prefer-props-shorthand` recommends shorthand syntax for props. Default: `warning`. Presets:
`nuxt`, `opinionated`.

`vue/prop-name-casing` enforces a casing for declared prop names. Default: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/require-component-registration` requires explicit component import or registration. Default:
`warning`. Presets: `opinionated`.

`vue/require-scoped-style` requires `scoped` on SFC style blocks. Default: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/scoped-event-names` recommends scoped event names such as `form:submit`. Default: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/sfc-element-order` enforces `vue/block-order`'s default order: `<script>` and `<template>` are
interchangeable, `<style>` last. Default: `warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/single-style-block` recommends keeping styles in a single block. Default: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/use-v-on-exact` enforces `.exact` when modifier-based handlers coexist. Default: `warning`.
Presets: `essential`, `nuxt`, `opinionated`.

`vue/v-bind-style`, `vue/v-on-style`, and `vue/v-slot-style` enforce directive style preferences.
Defaults: `warning`. Presets: `nuxt` and/or `happy-path`, plus `opinionated`.

`vue/valid-attribute-name`, `vue/valid-v-bind`, `vue/valid-v-else`, `vue/valid-v-for`,
`vue/valid-v-if`, `vue/valid-v-memo`, `vue/valid-v-model`, `vue/valid-v-on`, `vue/valid-v-show`,
and `vue/valid-v-slot` report invalid Vue directive syntax. Default: `error`. Presets:
`essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/warn-custom-block` and `vue/warn-custom-directive` warn about custom Vue extension points that
need host support or registration. Default: `warning`. Presets: `nuxt`, `opinionated`.
