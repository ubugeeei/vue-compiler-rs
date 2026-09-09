---
title: Règles Vue
---

<!-- Generated translation; source: rules/vue.md -->

# Règles Vue

Les règles Vue sont des règles Patina en file indienne. Ils inspectent la structure des modèles SFC, la syntaxe des directives, la dénomination des composants
et les dangers de correction spécifiques à Vue avant que le code n’atteigne l’exécution.

## `vue/require-v-for-key`

Ça exige que chaque `v-for` nœud ait une clé stable.

Sévérité par défaut : `error`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <li v-for="item in items">{{ item.name }}</li>
</template>
```

Bon :

```vue
<template>
  <li v-for="item in items" :key="item.id">{{ item.name }}</li>
</template>
```

## `vue/no-use-v-if-with-v-for`

Signale un nœud qui a `v-if` et `v-for` en même temps. Filtrer une valeur calculée maintient l’identité de la liste
stable et facilite l’analyse du modèle.

Sévérité par défaut : `warning`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <li v-for="item in items" v-if="item.visible" :key="item.id">
    {{ item.name }}
  </li>
</template>
```

Bon :

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

Rapports écrit aux accessoires. Le composant propriétaire doit mettre à jour la valeur via un événement ou un modèle
liaison.

Sévérité par défaut : `error`
Presets : `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<script setup lang="ts">
const props = defineProps<{ count: number }>();

props.count++;
</script>
```

Bon :

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

Les rapports `v-html` parce qu’ils rendent le HTML brut et peuvent transformer le contenu contrôlé par l’utilisateur en un puits XSS.

Sévérité par défaut : `warning`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <article v-html="content" />
</template>
```

Bon :

```vue
<template>
  <article>{{ content }}</article>
</template>
```

## `vue/no-child-content`

Signale le contenu enfant sur des éléments qui utilisent également `v-html` ou `v-text`. Vue remplace les enfants à
durée d’exécution, donc le contenu rédigé est trompeur.

Sévérité par défaut : `error`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <p v-text="message">Fallback text</p>
</template>
```

Bon :

```vue
<template>
  <p v-text="message" />
</template>
```

## `vue/no-duplicate-attributes`

Signale des attributs dupliqués sur le même élément.

Sévérité par défaut : `error`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <button class="primary" class="large">Save</button>
</template>
```

Bon :

```vue
<template>
  <button class="primary large">Save</button>
</template>
```

## `vue/no-dupe-v-else-if`

Signale des conditions répétées dans une chaîne `v-if` / `v-else-if` .

Sévérité par défaut : `error`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'ready'">Still ready</p>
</template>
```

Bon :

```vue
<template>
  <p v-if="status === 'ready'">Ready</p>
  <p v-else-if="status === 'loading'">Loading</p>
</template>
```

## `vue/no-template-shadow`

Rapporte des variables modèles qui ombragent les variables à partir d’un champ externe. Cela évite les références accidentelles
à une valeur différente de ce que le lecteur attend.

Sévérité par défaut : `warning`
Préréglages : `nuxt`, `opinionated`

Mauvais :

```vue
<script setup lang="ts">
const item = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

Bon :

```vue
<script setup lang="ts">
const selectedItem = ref("selected");
</script>

<template>
  <p v-for="item in items" :key="item.id">{{ item.name }}</p>
</template>
```

## `vue/no-unsafe-url`

Rapporte des liaisons URL et des attributs d’URL statiques qui peuvent se résoudre à des schémas dangereux tels que
`javascript:`, `vbscript:`ou des charges utiles `data:` exécutables.

Sévérité par défaut : `warning`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <iframe src="javascript:alert(1)"></iframe>
  <object data="data:text/html,<script>alert(1)</script>"></object>
  <img srcset="/safe.png 1x, javascript:alert(1) 2x" />
  <a :href="nextUrl">Continue</a>
</template>
```

Bon :

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

Signale des composants enregistrés localement qui n’apparaissent jamais dans le modèle.

Sévérité par défaut : `warning`
Presets : `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <p>{{ user.name }}</p>
</template>
```

Bon :

```vue
<script setup lang="ts">
import UserAvatar from "./UserAvatar.vue";
</script>

<template>
  <UserAvatar :user="user" />
</template>
```

## `vue/no-unused-properties`

Signale des props déclarés par `defineProps` qui ne sont pas utilisés par le composant.

Sévérité par défaut : `warning`
Presets : `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<script setup lang="ts">
defineProps<{ title: string; description: string }>();
</script>

<template>
  <h1>{{ title }}</h1>
</template>
```

Bon :

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

Les rapports `<component>` sans `is` contrainte.

Sévérité par défaut : `error`
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <component />
</template>
```

Bon :

```vue
<template>
  <component :is="currentComponent" />
</template>
```

## `vue/use-unique-element-ids`

Signale des identifiants littéraux statiques dans les endroits où `useId()` est plus sûr pour la réutilisation des composants et le SSR.

Sévérité par défaut : `warning`
Préréglages : `nuxt`, `opinionated`

Mauvais :

```vue
<template>
  <label for="email">Email</label>
  <input id="email" />
</template>
```

Bon :

```vue
<script setup lang="ts">
const emailId = useId();
</script>

<template>
  <label :for="emailId">Email</label>
  <input :id="emailId" />
</template>
```

## Syntaxe et règles de style

Ces règles n’ont pas besoin d’exemples longs, mais elles se comportent tout de même comme des règles de première classe et peuvent être
configurées par nom.

`vue/attribute-hyphenation` impose le style de nommage des attributs sur les composants personnalisés. Par défaut :
`warning`. Presets : `happy-path`, `nuxt`, `opinionated`.

`vue/attribute-order` impose un ordre d’attribut stable. Par défaut : `warning`. Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/component-definition-name-casing` impose les noms de définition des composants PascalCase. Par défaut :
`warning`. Presets : `happy-path`, `nuxt`, `opinionated`.

`vue/component-name-in-template-casing` impose la casse des noms de composants dans les modèles. Par défaut :
`warning`. Presets : `nuxt`, `opinionated`.

`vue/html-quotes` impose le style de guillemets pour les attributs HTML. Par défaut : `warning`. Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/html-self-closing` impose un style de fermeture automatique. Par défaut : `warning`. Presets : `nuxt`,
`opinionated`. Configurez `linter.ruleOptions["vue/html-self-closing"]` avec `html.void`,
`html.normal`, `html.component`, `svg` et `math`. Chaque valeur accepte `"always"`, `"never"` ou
`"any"`. Par défaut, Vize utilise `"always"` pour les éléments HTML void, les composants, SVG et
MathML, et `"any"` pour le HTML normal.

`vue/multi-word-component-names` exige que les noms des composants contiennent plus d’un mot. Par défaut :
`error`. Presets : `essential`, `nuxt`, `opinionated`.

`vue/mustache-interpolation-spacing` impose l’espacement à l’intérieur de l’interpolation de la moustache. Par défaut :
`warning`. Presets : `happy-path`, `nuxt`, `opinionated`.

`vue/no-boolean-attr-value` interdit les valeurs explicites pour les attributs HTML booléens. Par défaut :
`warning`. Presets : `nuxt`, `opinionated`.

`vue/no-inline-style` décourage les attributs de `style` en ligne. Par défaut : `warning`. Préréglages : `nuxt`,
`opinionated`.

`vue/no-lone-template` interdit les emballages de `<template>` inutiles. Par défaut : `warning`. Presets :
`happy-path`, `nuxt`, `opinionated`.

`vue/no-multi-spaces` interdit la répétition d’espaces dans les modèles. Par défaut : `warning`. Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/no-non-component-keep-alive-child` signale les wrappers d’élément natif directement sous
`<KeepAlive>`, car Vue ne peut mettre en cache que des VNode de composant. Par défaut : `warning`.
Presets : aucun (opt-in). Les wrappers avec seulement `v-show` sont ignorés.

`vue/no-preprocessor-lang` décourage les langages de préprocesseurs CSS dans les blocs SFC. Par défaut : `warning`.
Presets : `nuxt`, `opinionated`.

`vue/no-reserved-component-names` interdit de réserver des noms HTML ou Vue comme noms de composants. Par défaut :
`error`. Préréglages : `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-script-non-standard-lang` décourage les langages de script non standardisés. Par défaut : `warning`.
Presets : `nuxt`, `opinionated`.

`vue/no-src-attribute` décourage les attributs de `src` externes sur les blocs SFC. Par défaut : `warning`.
Presets : `nuxt`, `opinionated`.

`vue/no-template-key` interdit `key` sur `<template>`. Par défaut : `error`. Presets : `essential`,
`happy-path`, `nuxt`, `opinionated`.

`vue/no-template-lang` décourage `lang` sur `<template>`. Par défaut : `warning`. Préréglages : `nuxt`,
`opinionated`.

`vue/no-textarea-mustache` interdit l’interpolation de la moustache à l’intérieur de `<textarea>`. Par défaut : `error`.
Presets : `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-unused-vars` rapporte les variables inutilisées introduites par `v-for` et `v-slot`. Par défaut :
`warning`. Presets : `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-useless-template-attributes` interdit les attributs sur `<template>` que Vue ignore. Par défaut :
`error`. Préréglages : `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/no-v-text-v-html-on-component` interdit `v-text` ou `v-html` sur les éléments composants. Par défaut :
`error`. Presets : `essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/permitted-contents` impose des règles de modèles de contenu HTML à l’intérieur des modèles Vue. Par défaut : `error`.
Presets : `happy-path`, `nuxt`, `opinionated`.

`vue/prefer-props-shorthand` recommande une syntaxe abrégée pour les accessoires. Par défaut : `warning`. Préréglages :
`nuxt`, `opinionated`.

`vue/prop-name-casing` impose une casse (`camelCase` par défaut) pour les noms de props déclarés
via `defineProps` ; le côté modèle relève de `vue/attribute-hyphenation`. Par défaut : `warning`.
Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/require-component-registration` nécessite une importation ou un enregistrement explicite de composants. Par défaut :
`warning`. Presets : `opinionated`.

`vue/require-scoped-style` nécessite `scoped` sur des blocs de style SFC. Par défaut : `warning`. Presets :
`happy-path`, `nuxt`, `opinionated`.

`vue/scoped-event-names` recommande des noms d’événements à portée métrique tels que `form:submit`. Par défaut : `warning`.
Presets : `nuxt`, `opinionated`.

`vue/sfc-element-order` impose l’ordre des blocs SFC de niveau supérieur. Par défaut : `warning`. Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/single-style-block` recommande de garder les styles dans un seul bloc. Par défaut : `warning`. Préréglages :
`happy-path`, `nuxt`, `opinionated`.

`vue/use-v-on-exact` impose `.exact` lorsque les gestionnaires basés sur des modificateurs coexistent. Par défaut : `warning`.
Presets : `essential`, `nuxt`, `opinionated`.

`vue/v-bind-style`, `vue/v-on-style`, et `vue/v-slot-style` imposent des préférences de style directif.
Paramètres par défaut : `warning`. Préréglages : `nuxt` et/ou `happy-path`, plus `opinionated`.

`vue/valid-attribute-name`, `vue/valid-v-bind`, `vue/valid-v-else`, `vue/valid-v-for`,
`vue/valid-v-if`, `vue/valid-v-memo`, `vue/valid-v-model`, `vue/valid-v-on`, `vue/valid-v-show`,
et `vue/valid-v-slot` signalent une directive Vue invalide. Par défaut : `error`. Presets :
`essential`, `happy-path`, `nuxt`, `opinionated`.

`vue/warn-custom-block` et `vue/warn-custom-directive` avertissent des points d’extension personnalisés Vue qui
nécessiter un support hôte ou une inscription. Par défaut : `warning`. Presets : `nuxt`, `opinionated`.
