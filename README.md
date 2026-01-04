# vue-compiler-rs

A high-performance Rust implementation of the Vue.js compiler.

**[Playground](https://ubugeeei.github.io/vue-compiler-rs/)**

## Performance

Compiling **15,000 SFC files** (36.9 MB):

|  | @vue/compiler-sfc | vue-compiler-rs | Speedup |
|--|-------------------|-----------------|---------|
| **Single Thread** | 10.47s | 2.67s | **3.9x** |
| **Multi Thread** | 3.75s | 248ms | **15.1x** |

## Compatibility

Snapshot tests against `@vue/compiler-sfc` (v3.6.0-beta):

| Category | Passed | Total | Coverage |
|----------|--------|-------|----------|
| **VDom** | 226 | 338 | 66.9% |
| **Vapor** | 29 | 98 | 29.6% |
| **Total** | 255 | 436 | 58.5% |

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
cargo build -p vue_compiler_cli --release

# Compile single file
./target/release/vue-compiler "src/**/*.vue"

# Compile with output directory
./target/release/vue-compiler "src/**/*.vue" -o dist

# Show statistics only
./target/release/vue-compiler "src/**/*.vue" -f stats

# SSR mode
./target/release/vue-compiler "src/**/*.vue" --ssr

# Control thread count
./target/release/vue-compiler "src/**/*.vue" -j 4
```

Options:
- `-o, --output <DIR>` - Output directory (stdout if not specified)
- `-f, --format <FORMAT>` - Output format: `js`, `json`, `stats` (default: js)
- `-j, --threads <N>` - Number of threads (default: CPU count)
- `--ssr` - Enable SSR mode
- `--continue-on-error` - Continue on errors
- `-v, --verbose` - Verbose output

### Node.js / Browser

```javascript
// Node.js (Native)
const { compileSfc } = require('@vue-compiler-rs/native');
const { code } = compileSfc(`<template><div>{{ msg }}</div></template>`, { filename: 'App.vue' });

// Browser (WASM)
import init, { compileSfc } from '@vue-compiler-rs/wasm';
await init();
const { code } = compileSfc(`...`, { filename: 'App.vue' });
```

## License

MIT
