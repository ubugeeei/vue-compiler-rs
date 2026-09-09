---
title: Regras do Vue
---

<!-- Generated translation; source: rules/vue.md -->

# Regras do Vue

As regras do Vue são regras de Patina em fila única. Eles inspecionam a estrutura do template SFC, a sintaxe das diretivas, a nomeação
componentes e os riscos de correção específicos do Vue antes que o código chegue ao tempo de execução.

## `vue/require-v-for-key`

Exige que todo `v-for` nó tenha uma chave estável.

Gravidade padrão: `error`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
```

Bom:

```vue
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
```

## `vue/no-use-v-if-with-v-for`

Reporta um nó que `v-if` e `v-for` ao mesmo tempo. Filtrar um valor computado mantém a identidade da lista
estável e facilita a análise do modelo.

Gravidade padrão: `warning`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <li v-for="item in items" v-if="item.visible" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

Bom:

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

Relata para os adereços. O componente proprietário deve atualizar o valor por meio de um evento ou de um modelo
binding.

Gravidade padrão: `error`
Presets: `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();

props.count++;
</script>
```

Bom:

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

Reports `v-html` porque renderiza HTML bruto e pode transformar conteúdo controlado pelo usuário em um sumidouro XSS.

Gravidade padrão: `warning`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <article v-html="content" />
</template>
```

Bom:

```vue
<template>
  <article>{{ content }}</article>
</template>
```

## `vue/no-child-content`

Relata conteúdo filho sobre elementos que também usam `v-html` ou `v-text`. O Vue substitui as crianças em
tempo de execução, então o conteúdo criado é enganoso.

Gravidade padrão: `error`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <p v-text="message">Fallback text</p>
</template>
```

Bom:

```vue
<template>
  <p v-text="message" />
</template>
```

## `vue/no-duplicate-attributes`

Relata atributos duplicados no mesmo elemento.

Gravidade padrão: `error`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <button class="primary" class="large">Save</button>
</template>
```

Bom:

```vue
<template>
  <button class="primary large">Save</button>
</template>
```

## `vue/no-dupe-v-else-if`

Relata condições repetidas em uma cadeia de `v-if` / `v-else-if` .

Gravidade padrão: `error`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'ready'">Still ready</p>
</template>
```

Bom:

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'loading'">Loading</p>
</template>
```

## `vue/no-template-shadow`

Relatórios de variáveis modelo que ignoram variáveis de um escopo externo. Isso evita referências acidentais
a um valor diferente do que o leitor espera.

Gravidade padrão: `warning`
Presets: `nuxt`, `opinionated`

Ruim:

```vue
<script setup lang="ts">
const item = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

Bom:

```vue
<script setup lang="ts">
const selectedItem = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

## `vue/no-unsafe-url`

Relata vinculações de URL e atributos estáticos que podem se resolver em esquemas inseguros como
`javascript:`, `vbscript:`ou payloads executáveis `data:`.

Gravidade padrão: `warning`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <iframe src="javascript:alert(1)"></iframe>
  <object data="data:text/html,<script>alert(1)</script>"></object>
  <img srcset="/safe.png 1x, javascript:alert(1) 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

Bom:

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

Reporta componentes registrados localmente que nunca aparecem no modelo.

Gravidade padrão: `warning`
Presets: `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <p>{{ user.name }}</p>
</template>
```

Bom:

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <UserAvatar :user="user" />
</template>
```

## `vue/no-unused-properties`

Relata os props declarados por `defineProps` que não são usados pelo componente.

Gravidade padrão: `warning`
Presets: `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
</template>
```

Bom:

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

Os relatórios `<component>` sem `is` vinculação.

Gravidade padrão: `error`
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <component />
</template>
```

Bom:

```vue
<template>
  <component :is="currentComponent" />
</template>
```

## `vue/use-unique-element-ids`

Reporta IDs literais estáticos em locais onde `useId()` é mais seguro para reutilização de componentes e SSR.

Gravidade padrão: `warning`
Presets: `nuxt`, `opinionated`

Ruim:

```vue
<template>
  <label for="email">Email</label>
  <input id="email" />
</template>
```

Bom:

```vue
<script setup lang="ts">
const emailId = useId();
</script>

<template>
  <label :for="emailId">Email</label>
  <input :id="emailId" />
</template>
```

## Regras de Sintaxe e Estilo

Essas regras não precisam de exemplos longos, mas ainda assim se comportam como regras de primeira classe e podem ser
configuradas pelo nome.

`vue/attribute-hyphenation` impõe o estilo de nomeação de atributos em componentes personalizados. Padrão:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/attribute-order` impõe uma ordem de atributos estável. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/component-definition-name-casing` impõe nomes de definição de componentes do PascalCase. Padrão:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/component-name-in-template-casing` aplica a inclusão de nomes de componentes em templates. Padrão:
`warning`. Presets: `nuxt`, `opinionated`.

`vue/html-quotes` impõe o estilo de aspas para atributos HTML. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/html-self-closing` impõe um estilo de auto-fechamento. Padrão: `warning`. Presets: `nuxt`,
`opinionated`. Configure `linter.ruleOptions["vue/html-self-closing"]` com `html.void`,
`html.normal`, `html.component`, `svg` e `math`. Cada valor aceita `"always"`, `"never"` ou
`"any"`. Por padrão, o Vize usa `"always"` para HTML void, componentes, SVG e MathML, e `"any"`
para HTML normal.

`vue/multi-word-component-names` exige que os nomes dos componentes contenham mais de uma palavra. Padrão:
`error`. Presets: `essential`, `nuxt`, `opinionated`.

`vue/mustache-interpolation-spacing` impõe espaçamento dentro da interpolação do bigode. Padrão:
`warning`. Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/no-boolean-attr-value` não permite valores explícitos para atributos HTML booleanos. Padrão:
`warning`. Presets: `nuxt`, `opinionated`.

`vue/no-inline-style` desencoraja atributos de `style` em linha. Padrão: `warning`. Presets: `nuxt`,
`opinionated`.

`vue/no-lone-template` proíbe embalagens `<template>` desnecessárias. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/no-multi-spaces` proíbe espaços repetidos em templates. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/no-non-component-keep-alive-child` relata wrappers de elemento nativo diretamente sob
`<KeepAlive>`, porque Vue só consegue armazenar em cache VNodes de componente. Padrão: `warning`.
Presets: nenhum (opt-in). Wrappers somente com `v-show` são ignorados.

`vue/no-preprocessor-lang` desencoraja linguagens de pré-processador CSS em blocos SFC. Padrão: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-reserved-component-names` não permite nomes reservados de HTML ou Vue como nomes de componentes. Padrão:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-script-non-standard-lang` desencoraja linguagens de script não padrão. Padrão: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-src-attribute` desencoraja atributos externos de `src` nos blocos SFC. Padrão: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/no-template-key` proíbe `key` `<template>`. Padrão: `error`. Presets: `essential`,
`happy-path`, `nuxt`, `opinionated`.

`vue/no-template-lang` desencoraja `lang` em `<template>`. Padrão: `warning`. Presets: `nuxt`,
`opinionated`.

`vue/no-textarea-mustache` impede a interpolação do bigode dentro `<textarea>`. Padrão: `error`.
Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-unused-vars` reporta variáveis não utilizadas introduzidas por `v-for` e `v-slot`. Padrão:
`warning`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-useless-template-attributes` desabilita atributos em `<template>` que o Vue ignora. Padrão:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-v-text-v-html-on-component` não permite `v-text` ou `v-html` sobre elementos componentes. Padrão:
`error`. Presets: `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/permitted-contents` aplica regras de modelos de conteúdo HTML dentro dos templates do Vue. Padrão: `error`.
Presets: `happy-path`, `nuxt`, `opinionated`.

`vue/prefer-props-shorthand` recomenda sintaxe abreviada para adereços. Padrão: `warning`. Presets:
`nuxt`, `opinionated`.

`vue/prop-name-casing` impõe uma capitalização (`camelCase` por padrão) para os nomes de props
declarados via `defineProps`; o lado do modelo pertence a `vue/attribute-hyphenation`. Padrão:
`warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/require-component-registration` requer importação ou registro explícito de componentes. Padrão:
`warning`. Presets: `opinionated`.

`vue/require-scoped-style` exige `scoped` em blocos no estilo SFC. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/scoped-event-names` recomenda nomes de eventos com escopo, como `form:submit`. Padrão: `warning`.
Presets: `nuxt`, `opinionated`.

`vue/sfc-element-order` impõe a ordem dos blocos SFC de nível superior. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/single-style-block` recomenda manter os estilos em um único bloco. Padrão: `warning`. Presets:
`happy-path`, `nuxt`, `opinionated`.

`vue/use-v-on-exact` impõe `.exact` quando manipuladores baseados em modificadores coexistem. Padrão: `warning`.
Presets: `essential`, `nuxt`, `opinionated`.

`vue/v-bind-style`, `vue/v-on-style`e `vue/v-slot-style` impõem preferências de estilo diretivo.
Padrão: `warning`. Presets: `nuxt` e/ou `happy-path`, mais `opinionated`.

`vue/valid-attribute-name`, `vue/valid-v-bind`, `vue/valid-v-else`, `vue/valid-v-for`,
`vue/valid-v-if`, `vue/valid-v-memo`, `vue/valid-v-model`, `vue/valid-v-on`, `vue/valid-v-show`,
e `vue/valid-v-slot` reportam a sintaxe inválida da diretiva do Vue. Padrão: `error`. Presets:
`essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/warn-custom-block` e `vue/warn-custom-directive` alertam sobre pontos de extensão personalizados do Vue que
precisam de suporte ou registro de hosts. Padrão: `warning`. Presets: `nuxt`, `opinionated`.
