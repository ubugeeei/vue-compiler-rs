<p align="center">
  <img src="./assets/logo.svg" alt="Vize Logo" width="400" />
</p>

<h1 align="center">Vize</h1>

<p align="center">
  <strong>Unofficial Fastest Vue.js Compiler Collection</strong>
</p>

<p align="center">
  <em>A high-performance Rust implementation of the Vue.js compiler.<br/>Named after Vizier + Visor + Advisor — a wise tool that sees through your code.</em>
</p>

<p align="center">
  <a href="https://ubugeeei.github.io/vize/"><strong>Playground</strong></a>
</p>

---

## Performance

Compiling **15,000 SFC files** (36.9 MB):

|  | @vue/compiler-sfc | Vize | Speedup |
|--|-------------------|-----------------|---------|
| **Single Thread** | 14.13s | 6.77s | **2.1x** |
| **Multi Thread** (10 workers) | 3.97s | 593ms | **6.7x** |

## Compatibility

Snapshot tests against `@vue/compiler-sfc` (v3.6.0-beta):

| Category | Passed | Total | Coverage |
|----------|--------|-------|----------|
| **VDom** | 267 | 338 | 79.0% |
| **Vapor** | 29 | 98 | 29.6% |
| **SFC** | 3 | 70 | 4.3% |
| **Total** | 299 | 506 | 59.1% |

### TypeScript Output Snapshots

We maintain **70 snapshot tests** for TypeScript output mode in `tests/snapshots/sfc/ts/`. These capture the current behavior for:

- Basic script setup patterns
- defineProps/defineEmits/defineModel
- Props destructure with defaults
- Generic components (Vue 3.3+)
- Complex TypeScript types (arrow functions, unions, intersections)
- Top-level await
- withDefaults patterns
- Real-world patterns from production codebases

Run `mise run snapshot` to update snapshots after changes.

### CLI Output Modes

The CLI supports two output modes via `--script-ext`:

- `downcompile` (default): Transpiles TypeScript to JavaScript
- `preserve`: Keeps TypeScript output as-is

```bash
# Preserve TypeScript output (recommended for TypeScript projects)
vize "src/**/*.vue" --script-ext preserve -o dist

# Downcompile to JavaScript (default)
vize "src/**/*.vue" -o dist
```

### Known Limitations

Some Vue 3.3+ features are not yet fully supported:
- Generic component declarations (`<script setup generic="T">`)
- `as const` assertions in multiline expressions

### Recent Improvements

- **TypeScript Interface Resolution**: `defineProps<Props>()` now correctly resolves interface and type alias references defined in the same file
- **Props Destructure Defaults**: Default values in props destructure patterns are properly handled
- **withDefaults Support**: `withDefaults(defineProps<Props>(), { ... })` works correctly with interface references
- **Downcompile Mode Fix**: TypeScript files are now correctly transpiled to JavaScript in downcompile mode (default), including complex patterns like `ref<HTMLElement | null>(null)`
- **Component Resolution**: Custom components like `v-btn` now correctly generate `resolveComponent` calls in inline templates
- **v-on Object Spread**: `v-on="handlers"` object spread syntax is now correctly compiled using `toHandlers`
- **$emit Prefixing**: `$emit` and other Vue instance properties are now correctly prefixed with `_ctx.` in template expressions
- **Normal Script Transpilation**: When using both `<script>` and `<script setup>`, the normal script block is now also transpiled to JavaScript in downcompile mode

## Quick Start

```bash
mise install && mise run setup
mise run build    # Build bindings
mise run test     # Run tests
mise run cov      # Coverage report
mise run dev      # Playground
```

Run `mise tasks` to see all available commands.

## Usage

### CLI

```bash
# Build CLI
cargo build -p vize_compiler_cli --release

# Compile single file
./target/release/vize "src/**/*.vue"

# Compile with output directory
./target/release/vize "src/**/*.vue" -o dist

# Show statistics only
./target/release/vize "src/**/*.vue" -f stats

# SSR mode
./target/release/vize "src/**/*.vue" --ssr

# Control thread count
./target/release/vize "src/**/*.vue" -j 4
```

Options:
- `-o, --output <DIR>` - Output directory (stdout if not specified)
- `-f, --format <FORMAT>` - Output format: `js`, `json`, `stats` (default: js)
- `-j, --threads <N>` - Number of threads (default: CPU count)
- `--script-ext <MODE>` - Script extension handling: `preserve` or `downcompile` (default: downcompile)
- `--ssr` - Enable SSR mode
- `--continue-on-error` - Continue on errors
- `--profile` - Show timing profile breakdown

### Node.js / Browser

```javascript
// Node.js (Native)
const { compileSfc } = require('@vize/native');
const { code } = compileSfc(`<template><div>{{ msg }}</div></template>`, { filename: 'App.vue' });

// Browser (WASM)
import init, { compileSfc } from '@vize/wasm';
await init();
const { code } = compileSfc(`...`, { filename: 'App.vue' });
```

## Roadmap

| Crate | Description | Status |
|-------|-------------|--------|
| `vize_compiler_sfc` | SFC Compiler | In Progress |
| `vize_compiler_vapor` | Vapor Mode Compiler | In Progress |
| `vize_typechecker` | TypeScript Type Checker | Planned |
| `vize_linter` | Vue.js Linter | Planned |
| `vize_formatter` | Vue.js Formatter | Planned |

## License

MIT
