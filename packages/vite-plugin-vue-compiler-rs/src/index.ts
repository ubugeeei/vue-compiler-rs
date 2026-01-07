import type { Plugin, ResolvedConfig } from 'vite';
import { createHash } from 'node:crypto';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

export interface VueCompilerRsOptions {
  include?: string | RegExp | (string | RegExp)[];
  exclude?: string | RegExp | (string | RegExp)[];
  isProduction?: boolean;
  ssr?: boolean;
  sourceMap?: boolean;
  vapor?: boolean;
}

interface SfcCompileResult {
  descriptor: {
    filename: string;
    source: string;
    template?: { content: string };
    script?: { content: string; setup: boolean };
    scriptSetup?: { content: string; setup: boolean };
    styles: Array<{ content: string; scoped: boolean }>;
  };
  script: {
    code: string;
    bindings?: Record<string, string>;
  };
  css?: string;
  errors: string[];
  warnings: string[];
}

interface WasmModule {
  compileSfc: (source: string, options: Record<string, unknown>) => SfcCompileResult;
}

let wasmModule: WasmModule | null = null;

function loadWasm(): WasmModule {
  if (wasmModule) return wasmModule;

  try {
    // Load CommonJS WASM module using require (Node.js target)
    const wasmJsPath = path.resolve(__dirname, '../wasm/vue_bindings.js');
    const wasmBinaryPath = path.resolve(__dirname, '../wasm/vue_bindings_bg.wasm');

    if (fs.existsSync(wasmJsPath) && fs.existsSync(wasmBinaryPath)) {
      const wasmModule_ = require(wasmJsPath);
      const wasmBuffer = fs.readFileSync(wasmBinaryPath);

      // Initialize the WASM module
      if (wasmModule_.initSync) {
        wasmModule_.initSync({ module: wasmBuffer });
      }

      // Extract the exported functions
      wasmModule = {
        compileSfc: wasmModule_.compileSfc,
      } as WasmModule;
      return wasmModule;
    }
    throw new Error('WASM module not found at: ' + wasmJsPath);
  } catch (e) {
    throw new Error(`Failed to load vue-compiler-rs WASM: ${e}`);
  }
}

function generateScopeId(filename: string): string {
  const hash = createHash('sha256').update(filename).digest('hex');
  return hash.slice(0, 8);
}

function createFilter(
  include?: string | RegExp | (string | RegExp)[],
  exclude?: string | RegExp | (string | RegExp)[]
): (id: string) => boolean {
  const includePatterns = include
    ? (Array.isArray(include) ? include : [include])
    : [/\.vue$/];
  const excludePatterns = exclude
    ? (Array.isArray(exclude) ? exclude : [exclude])
    : [/node_modules/];

  return (id: string) => {
    const matchInclude = includePatterns.some(pattern =>
      typeof pattern === 'string' ? id.includes(pattern) : pattern.test(id)
    );
    const matchExclude = excludePatterns.some(pattern =>
      typeof pattern === 'string' ? id.includes(pattern) : pattern.test(id)
    );
    return matchInclude && !matchExclude;
  };
}

export function vueCompilerRs(options: VueCompilerRsOptions = {}): Plugin {
  const filter = createFilter(options.include, options.exclude);
  let config: ResolvedConfig;
  let isProduction = options.isProduction ?? false;

  return {
    name: 'vite-plugin-vue-compiler-rs',
    enforce: 'pre',

    configResolved(resolvedConfig: ResolvedConfig) {
      config = resolvedConfig;
      isProduction = options.isProduction ?? config.isProduction;
    },

    async resolveId(id: string) {
      // Handle virtual modules for styles
      if (id.includes('.vue?vue&type=style')) {
        return id;
      }
      return null;
    },

    load(id: string) {
      // Handle virtual style modules
      if (id.includes('.vue?vue&type=style')) {
        const [filename] = id.split('?');
        const source = fs.readFileSync(filename, 'utf-8');
        const wasm = loadWasm();

        const result = wasm.compileSfc(source, {
          filename,
          mode: 'module',
          sourceMap: options.sourceMap ?? !isProduction,
          outputMode: options.vapor ? 'vapor' : 'vdom',
        });

        return result.css || '';
      }
      return null;
    },

    transform(code: string, id: string) {
      if (!filter(id)) return null;
      if (!id.endsWith('.vue')) return null;

      const wasm = loadWasm();
      const scopeId = generateScopeId(id);
      const hasScoped = /<style[^>]*\bscoped\b/.test(code);

      try {
        const result = wasm.compileSfc(code, {
          filename: id,
          mode: 'module',
          scopeId: hasScoped ? `data-v-${scopeId}` : undefined,
          sourceMap: options.sourceMap ?? !isProduction,
          ssr: options.ssr ?? false,
          outputMode: options.vapor ? 'vapor' : 'vdom',
        });

        if (result.errors.length > 0) {
          throw new Error(result.errors.join('\n'));
        }

        let output = result.script.code;

        // Inject CSS
        if (result.css) {
          const cssCode = JSON.stringify(result.css);
          output = `
const __css__ = ${cssCode};
(function() {
  if (typeof document !== 'undefined') {
    const style = document.createElement('style');
    style.textContent = __css__;
    document.head.appendChild(style);
  }
})();
${output}`;
        }

        // Add HMR support
        if (!isProduction && config?.command === 'serve') {
          output += `
if (import.meta.hot) {
  import.meta.hot.accept(mod => {
    if (!mod) return;
    const { default: updated } = mod;
    __sfc__.__hmrId = ${JSON.stringify(scopeId)};
    __VUE_HMR_RUNTIME__.reload(__sfc__.__hmrId, updated);
  });
  __sfc__.__hmrId = ${JSON.stringify(scopeId)};
  if (typeof __VUE_HMR_RUNTIME__ !== 'undefined') {
    __VUE_HMR_RUNTIME__.createRecord(__sfc__.__hmrId, __sfc__);
  }
}`;
        }

        return {
          code: output,
          map: null,
        };
      } catch (e) {
        this.error(`[vue-compiler-rs] ${e}`);
      }
    },
  };
}

export default vueCompilerRs;
