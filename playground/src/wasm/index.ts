// WASM module loader for vue-compiler-rs

export interface CompilerOptions {
  mode?: 'function' | 'module';
  ssr?: boolean;
  scopeId?: string | null;
  filename?: string;
  // Internal mock-only property for vapor mode detection
  outputMode?: 'vdom' | 'vapor';
}

export interface CompileResult {
  code: string;
  preamble: string;
  ast: object;
  map?: object | null;
  helpers: string[];
  templates?: string[];
}

export interface SfcBlock {
  content: string;
  loc: { start: number; end: number };
  lang?: string;
  src?: string;
  attrs: Record<string, string>;
}

export interface SfcScriptBlock extends SfcBlock {
  setup: boolean;
}

export interface SfcStyleBlock extends SfcBlock {
  scoped: boolean;
  module?: string;
}

export interface SfcDescriptor {
  filename: string;
  source: string;
  template?: SfcBlock;
  script?: SfcScriptBlock;
  scriptSetup?: SfcScriptBlock;
  styles: SfcStyleBlock[];
  customBlocks: Array<{ type: string; content: string; attrs: Record<string, string> }>;
}

export interface SfcCompileResult {
  descriptor: SfcDescriptor;
  template?: CompileResult;
  script?: {
    code: string;
    bindings?: object;
  };
  css?: string;
  errors?: string[];
  warnings?: string[];
  bindingMetadata?: object;
}

export interface CssCompileOptions {
  scopeId?: string;
  scoped?: boolean;
  minify?: boolean;
  sourceMap?: boolean;
  filename?: string;
  targets?: {
    chrome?: number;
    firefox?: number;
    safari?: number;
    edge?: number;
    ios?: number;
    android?: number;
  };
}

export interface CssCompileResult {
  code: string;
  map?: string;
  cssVars: string[];
  errors: string[];
  warnings: string[];
}

export interface WasmModule {
  compile: (template: string, options: CompilerOptions) => CompileResult;
  compileVapor: (template: string, options: CompilerOptions) => CompileResult;
  compileCss: (css: string, options: CssCompileOptions) => CssCompileResult;
  parseTemplate: (template: string, options: CompilerOptions) => object;
  parseSfc: (source: string, options: CompilerOptions) => SfcDescriptor;
  compileSfc: (source: string, options: CompilerOptions) => SfcCompileResult;
  Compiler: new () => {
    compile: (template: string, options: CompilerOptions) => CompileResult;
    compileVapor: (template: string, options: CompilerOptions) => CompileResult;
    compileCss: (css: string, options: CssCompileOptions) => CssCompileResult;
    parse: (template: string, options: CompilerOptions) => object;
    parseSfc: (source: string, options: CompilerOptions) => SfcDescriptor;
    compileSfc: (source: string, options: CompilerOptions) => SfcCompileResult;
  };
}

let wasmModule: WasmModule | null = null;
let loadPromise: Promise<WasmModule> | null = null;

export async function loadWasm(): Promise<WasmModule> {
  if (wasmModule) {
    return wasmModule;
  }

  if (loadPromise) {
    return loadPromise;
  }

  loadPromise = (async () => {
    try {
      // Try to load the actual WASM module
      const wasm = await import('./vue_bindings.js');
      await wasm.default();
      wasmModule = wasm as unknown as WasmModule;
      return wasmModule;
    } catch (e) {
      console.warn('WASM module not found, using mock compiler:', e);
      // Return mock module if WASM is not available
      wasmModule = createMockModule();
      return wasmModule;
    }
  })();

  return loadPromise;
}

// Mock module for development when WASM is not built
function createMockModule(): WasmModule {
  const mockCompile = (template: string, options: CompilerOptions): CompileResult => {
    const isVapor = options.outputMode === 'vapor';
    const hasInterpolation = template.includes('{{');
    const hasElement = template.includes('<');
    const hasVIf = template.includes('v-if');
    const hasVFor = template.includes('v-for');
    const hasVOn = template.includes('@') || template.includes('v-on');

    let code = '';
    const helpers: string[] = [];

    if (isVapor) {
      // Vapor mode output
      code = `// Vapor Mode Output\n`;
      code += `import { template, createTextNode, setText, renderEffect } from 'vue/vapor';\n\n`;
      code += `const t0 = template(\`${template.replace(/\{\{[^}]+\}\}/g, '<!>')}\`);\n\n`;
      code += `export function render(_ctx) {\n`;
      code += `  const n0 = t0();\n`;

      if (hasInterpolation) {
        code += `  const x0 = createTextNode();\n`;
        code += `  renderEffect(() => setText(x0, _ctx.msg));\n`;
        helpers.push('template', 'createTextNode', 'setText', 'renderEffect');
      }

      code += `  return n0;\n`;
      code += `}`;
    } else {
      // VDom mode output
      helpers.push('createElementVNode');
      if (hasInterpolation) helpers.push('toDisplayString');
      if (hasVIf) helpers.push('createCommentVNode', 'Fragment', 'openBlock', 'createElementBlock');
      if (hasVFor) helpers.push('renderList', 'Fragment', 'openBlock', 'createElementBlock');
      if (hasVOn) helpers.push('withModifiers');

      const isModule = options.mode === 'module';

      if (isModule) {
        code = `import { ${helpers.join(', ')} } from "vue"\n\n`;
        code += `export function render(_ctx, _cache, $props, $setup, $data, $options) {\n`;
      } else {
        code = `function render(_ctx, _cache) {\n`;
      }

      code += `  return `;

      if (hasElement) {
        const tagMatch = template.match(/<(\w+)/);
        const tag = tagMatch ? tagMatch[1] : 'div';

        if (hasVIf) {
          code += `(_ctx.show)\n`;
          code += `    ? (_openBlock(), _createElementBlock("${tag}", { key: 0 }))\n`;
          code += `    : _createCommentVNode("v-if", true)`;
        } else if (hasVFor) {
          code += `(_openBlock(true), _createElementBlock(_Fragment, null, _renderList(_ctx.items, (item) => {\n`;
          code += `    return (_openBlock(), _createElementBlock("${tag}", { key: item.id }))\n`;
          code += `  }), 128))`;
        } else if (hasInterpolation) {
          const expr = template.match(/\{\{\s*([^}]+)\s*\}\}/)?.[1] || 'msg';
          code += `_createElementVNode("${tag}", null, _toDisplayString(_ctx.${expr.trim()}), 1 /* TEXT */)`;
        } else {
          code += `_createElementVNode("${tag}", null, null, -1 /* HOISTED */)`;
        }
      } else if (hasInterpolation) {
        const expr = template.match(/\{\{\s*([^}]+)\s*\}\}/)?.[1] || '';
        code += `_toDisplayString(_ctx.${expr.trim()})`;
      } else {
        code += `"${template}"`;
      }

      code += `\n}`;
    }

    // Build AST representation
    const ast = buildMockAst(template);

    return {
      code,
      preamble: options.mode === 'function' ? `const { ${helpers.join(', ')} } = Vue\n` : '',
      ast,
      helpers,
    };
  };

  const mockParseSfc = (source: string, _options: CompilerOptions): SfcDescriptor => {
    const templateMatch = source.match(/<template>([\s\S]*?)<\/template>/);
    const scriptMatch = source.match(/<script>([\s\S]*?)<\/script>/);
    const scriptSetupMatch = source.match(/<script\s+setup>([\s\S]*?)<\/script>/);
    const styleMatches = [...source.matchAll(/<style(\s+scoped)?>([\s\S]*?)<\/style>/g)];

    return {
      filename: 'anonymous.vue',
      source,
      template: templateMatch ? {
        content: templateMatch[1],
        loc: { start: 0, end: 0 },
        attrs: {},
      } : undefined,
      script: scriptMatch ? {
        content: scriptMatch[1],
        loc: { start: 0, end: 0 },
        attrs: {},
        setup: false,
      } : undefined,
      scriptSetup: scriptSetupMatch ? {
        content: scriptSetupMatch[1],
        loc: { start: 0, end: 0 },
        attrs: {},
        setup: true,
      } : undefined,
      styles: styleMatches.map(m => ({
        content: m[2],
        loc: { start: 0, end: 0 },
        attrs: {},
        scoped: !!m[1],
      })),
      customBlocks: [],
    };
  };

  const mockCompileSfc = (source: string, options: CompilerOptions): SfcCompileResult => {
    const descriptor = mockParseSfc(source, options);
    const templateResult = descriptor.template
      ? mockCompile(descriptor.template.content, options)
      : undefined;

    // Generate mock script code and extract bindings
    let scriptCode = '';
    const bindings: Record<string, string> = {};

    if (descriptor.scriptSetup) {
      scriptCode = `// Mock compiled script setup\n`;
      scriptCode += `import { ref, computed } from 'vue'\n\n`;
      scriptCode += `export default {\n`;
      scriptCode += `  setup() {\n`;
      scriptCode += `    ${descriptor.scriptSetup.content.split('\n').filter(l => !l.trim().startsWith('import')).join('\n    ')}\n`;
      scriptCode += `    return { /* bindings */ }\n`;
      scriptCode += `  }\n`;
      scriptCode += `}\n`;
      if (templateResult) {
        scriptCode += `\n${templateResult.code}`;
      }

      // Extract mock bindings from script setup content
      const lines = descriptor.scriptSetup.content.split('\n');
      for (const line of lines) {
        // Match: const x = ref(...), const x = computed(...), or defineProps destructure
        const constMatch = line.match(/const\s+(\w+)\s*=/);
        if (constMatch) {
          const varName = constMatch[1];
          if (line.includes('ref(')) {
            bindings[varName] = 'setup-ref';
          } else if (line.includes('computed(')) {
            bindings[varName] = 'setup-computed';
          } else {
            bindings[varName] = 'setup-const';
          }
        }
      }
    } else if (descriptor.script) {
      scriptCode = descriptor.script.content;
    } else {
      scriptCode = 'export default {}';
    }

    return {
      descriptor,
      template: templateResult,
      script: { code: scriptCode },
      css: descriptor.styles.map(s => s.content).join('\n') || undefined,
      bindingMetadata: Object.keys(bindings).length > 0 ? bindings : undefined,
    };
  };

  const mockCompileCss = (css: string, options: CssCompileOptions): CssCompileResult => {
    let code = css;

    // Apply scoping if requested
    if (options.scoped && options.scopeId) {
      // Simple mock scoping - just append attribute selector
      code = css.replace(/([.#]?\w+)(\s*\{)/g, `$1[${options.scopeId}]$2`);
    }

    // Minify if requested
    if (options.minify) {
      code = code
        .replace(/\s+/g, ' ')
        .replace(/\s*{\s*/g, '{')
        .replace(/\s*}\s*/g, '}')
        .replace(/\s*:\s*/g, ':')
        .replace(/\s*;\s*/g, ';')
        .trim();
    }

    // Extract v-bind() expressions
    const cssVars: string[] = [];
    const vBindRegex = /v-bind\(([^)]+)\)/g;
    let match;
    while ((match = vBindRegex.exec(css)) !== null) {
      const expr = match[1].trim().replace(/['"]/g, '');
      cssVars.push(expr);
    }

    return {
      code,
      cssVars,
      errors: [],
      warnings: [],
    };
  };

  class MockCompiler {
    compile(template: string, options: CompilerOptions): CompileResult {
      return mockCompile(template, options);
    }
    compileVapor(template: string, options: CompilerOptions): CompileResult {
      return mockCompile(template, { ...options, outputMode: 'vapor' });
    }
    compileCss(css: string, options: CssCompileOptions): CssCompileResult {
      return mockCompileCss(css, options);
    }
    parse(template: string, _options: CompilerOptions): object {
      return buildMockAst(template);
    }
    parseSfc(source: string, options: CompilerOptions): SfcDescriptor {
      return mockParseSfc(source, options);
    }
    compileSfc(source: string, options: CompilerOptions): SfcCompileResult {
      return mockCompileSfc(source, options);
    }
  }

  return {
    compile: mockCompile,
    compileVapor: (template: string, options: CompilerOptions) =>
      mockCompile(template, { ...options, outputMode: 'vapor' }),
    compileCss: mockCompileCss,
    parseTemplate: (template: string) => buildMockAst(template),
    parseSfc: mockParseSfc,
    compileSfc: mockCompileSfc,
    Compiler: MockCompiler,
  };
}

function buildMockAst(template: string): object {
  const children: object[] = [];

  // Simple regex-based parsing for mock AST
  const elementMatch = template.match(/<(\w+)([^>]*)>([\s\S]*?)<\/\1>/);
  if (elementMatch) {
    const [, tag, attrs, content] = elementMatch;
    const props: object[] = [];

    // Parse attributes
    const vIfMatch = attrs.match(/v-if="([^"]+)"/);
    if (vIfMatch) {
      props.push({ type: 'DIRECTIVE', name: 'if', exp: vIfMatch[1] });
    }

    const vForMatch = attrs.match(/v-for="([^"]+)"/);
    if (vForMatch) {
      props.push({ type: 'DIRECTIVE', name: 'for', exp: vForMatch[1] });
    }

    const vOnMatch = attrs.match(/@(\w+)="([^"]+)"/);
    if (vOnMatch) {
      props.push({ type: 'DIRECTIVE', name: 'on', arg: vOnMatch[1], exp: vOnMatch[2] });
    }

    const vBindMatch = attrs.match(/:(\w+)="([^"]+)"/);
    if (vBindMatch) {
      props.push({ type: 'DIRECTIVE', name: 'bind', arg: vBindMatch[1], exp: vBindMatch[2] });
    }

    const classMatch = attrs.match(/class="([^"]+)"/);
    if (classMatch) {
      props.push({ type: 'ATTRIBUTE', name: 'class', value: classMatch[1] });
    }

    // Parse children
    const childElements: object[] = [];
    const interpMatch = content.match(/\{\{\s*([^}]+)\s*\}\}/g);
    if (interpMatch) {
      interpMatch.forEach((interp) => {
        const exp = interp.match(/\{\{\s*([^}]+)\s*\}\}/)?.[1] || '';
        childElements.push({
          type: 'INTERPOLATION',
          content: { type: 'SIMPLE_EXPRESSION', content: exp.trim() },
        });
      });
    }

    children.push({
      type: 'ELEMENT',
      tag,
      tagType: 'ELEMENT',
      props,
      children: childElements,
      isSelfClosing: false,
    });
  }

  return {
    type: 'ROOT',
    children,
    helpers: [],
    components: [],
    directives: [],
    hoists: [],
    imports: [],
    cached: [],
  };
}

export function isWasmLoaded(): boolean {
  return wasmModule !== null;
}
