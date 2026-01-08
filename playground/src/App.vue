<script setup lang="ts">
import { ref, computed, watch, onMounted, shallowRef } from 'vue';
import MonacoEditor from './components/MonacoEditor.vue';
import CodeHighlight from './components/CodeHighlight.vue';
import { PRESETS, type PresetKey, type InputMode } from './presets';
import { loadWasm, isWasmLoaded, isUsingMock, type CompilerOptions, type CompileResult, type SfcCompileResult, type CssCompileResult, type CssCompileOptions } from './wasm/index';
import * as prettier from 'prettier/standalone';
import * as parserBabel from 'prettier/plugins/babel';
import * as parserEstree from 'prettier/plugins/estree';
import * as parserTypescript from 'prettier/plugins/typescript';
import * as parserCss from 'prettier/plugins/postcss';
import ts from 'typescript';

// Convert Map objects to plain objects recursively (for serde_wasm_bindgen output)
function mapToObject(value: unknown): unknown {
  if (value instanceof Map) {
    const obj: Record<string, unknown> = {};
    value.forEach((v, k) => {
      obj[String(k)] = mapToObject(v);
    });
    return obj;
  }
  if (Array.isArray(value)) {
    return value.map(mapToObject);
  }
  if (value !== null && typeof value === 'object') {
    const obj: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      obj[k] = mapToObject(v);
    }
    return obj;
  }
  return value;
}

type TabType = 'code' | 'ast' | 'helpers' | 'sfc' | 'css' | 'bindings';

// State
const inputMode = ref<InputMode>('sfc');
const source = ref(PRESETS.propsDestructure.code);
const output = ref<CompileResult | null>(null);
const sfcResult = ref<SfcCompileResult | null>(null);
const error = ref<string | null>(null);
const options = ref<CompilerOptions>({
  mode: 'module',
  ssr: false,
  scriptExt: 'preserve', // Keep TypeScript types in output
});
const activeTab = ref<TabType>('code');
const isCompiling = ref(false);
const wasmStatus = ref<'loading' | 'ready' | 'mock'>('loading');
const selectedPreset = ref<PresetKey>('propsDestructure');
const compileTime = ref<number | null>(null);
const cssResult = ref<CssCompileResult | null>(null);
const cssOptions = ref<CssCompileOptions>({
  scoped: false,
  scopeId: 'data-v-12345678',
  minify: false,
});
const compiler = shallowRef<Awaited<ReturnType<typeof loadWasm>> | null>(null);
const formattedCode = ref<string>('');
const formattedCss = ref<string>('');
const formattedJsCode = ref<string>('');
const codeViewMode = ref<'ts' | 'js'>('ts');

// Helper to format code with Prettier
async function formatCode(code: string, parser: 'babel' | 'typescript'): Promise<string> {
  try {
    return await prettier.format(code, {
      parser,
      plugins: [parserBabel, parserEstree, parserTypescript],
      semi: false,
      singleQuote: true,
      printWidth: 80,
    });
  } catch {
    return code;
  }
}

async function formatCss(code: string): Promise<string> {
  try {
    return await prettier.format(code, {
      parser: 'css',
      plugins: [parserCss],
      printWidth: 80,
    });
  } catch {
    return code;
  }
}

// Transpile TypeScript to JavaScript
function transpileToJs(code: string): string {
  try {
    const result = ts.transpileModule(code, {
      compilerOptions: {
        module: ts.ModuleKind.ESNext,
        target: ts.ScriptTarget.ESNext,
        jsx: ts.JsxEmit.Preserve,
        removeComments: false,
      },
    });
    return result.outputText;
  } catch {
    return code;
  }
}

// Computed
const editorLanguage = computed(() => inputMode.value === 'sfc' ? 'vue' : 'html');
const astJson = computed(() => output.value ? JSON.stringify(mapToObject(output.value.ast), null, 2) : '{}');

// Computed: detect TypeScript from script lang
const isTypeScript = computed(() => {
  if (!sfcResult.value?.descriptor) return false;
  const scriptSetup = sfcResult.value.descriptor.scriptSetup;
  const script = sfcResult.value.descriptor.script;
  const lang = scriptSetup?.lang || script?.lang;
  return lang === 'ts' || lang === 'tsx';
});

// Methods
async function compile() {
  if (!compiler.value) return;

  isCompiling.value = true;
  error.value = null;

  try {
    const startTime = performance.now();

    if (inputMode.value === 'sfc') {
      try {
        const result = compiler.value.compileSfc(source.value, options.value);
        compileTime.value = performance.now() - startTime;
        sfcResult.value = result;

        // Compile CSS from all style blocks
        if (result?.descriptor?.styles?.length > 0) {
          const allCss = result.descriptor.styles.map(s => s.content).join('\n');
          const hasScoped = result.descriptor.styles.some(s => s.scoped);
          const css = compiler.value.compileCss(allCss, {
            ...cssOptions.value,
            scoped: hasScoped || cssOptions.value.scoped,
          });
          cssResult.value = css;
          // Format CSS
          formattedCss.value = await formatCss(css.code);
        } else {
          cssResult.value = null;
          formattedCss.value = '';
        }

        if (result?.script?.code) {
          output.value = {
            code: result.script.code,
            preamble: result.template?.preamble || '',
            ast: result.template?.ast || {},
            helpers: result.template?.helpers || [],
          };
          // Detect TypeScript from script lang
          const scriptLang = result.descriptor.scriptSetup?.lang || result.descriptor.script?.lang;
          const usesTs = scriptLang === 'ts' || scriptLang === 'tsx';
          console.log('scriptLang:', scriptLang, 'usesTs:', usesTs);
          console.log('raw code:', result.script.code);
          // Format code with appropriate parser
          formattedCode.value = await formatCode(result.script.code, usesTs ? 'typescript' : 'babel');
          console.log('formattedCode:', formattedCode.value);
          // Also generate JS version for TypeScript
          if (usesTs) {
            const jsCode = transpileToJs(result.script.code);
            formattedJsCode.value = await formatCode(jsCode, 'babel');
          } else {
            formattedJsCode.value = '';
          }
        } else if (result?.template) {
          output.value = result.template;
          formattedCode.value = await formatCode(result.template.code, 'babel');
          formattedJsCode.value = '';
        } else {
          output.value = null;
          formattedCode.value = '';
          formattedJsCode.value = '';
        }
      } catch (sfcError) {
        console.error('SFC compile error:', sfcError);
        throw sfcError;
      }
    } else {
      const result = compiler.value.compile(source.value, options.value);
      compileTime.value = performance.now() - startTime;
      output.value = result;
      sfcResult.value = null;
      formattedCode.value = await formatCode(result.code, 'babel');
      formattedCss.value = '';
    }
  } catch (e) {
    const errorMessage = e instanceof Error ? e.message : String(e);
    error.value = errorMessage;
  } finally {
    isCompiling.value = false;
  }
}

function handlePresetChange(key: PresetKey) {
  const preset = PRESETS[key];
  selectedPreset.value = key;
  inputMode.value = preset.mode;
  source.value = preset.code;
  if (preset.mode === 'sfc') {
    activeTab.value = 'code';
  }
}

function copyToClipboard(text: string) {
  navigator.clipboard.writeText(text);
}

function copyFullOutput() {
  if (!output.value) return;
  const fullOutput = `
=== COMPILER OUTPUT ===
Compile Time: ${compileTime?.value?.toFixed(2) ?? 'N/A'}ms

=== CODE ===
${output.value.code}

=== HELPERS ===
${output.value.helpers?.join('\n') || 'None'}
`.trim();
  copyToClipboard(fullOutput);
}

// Watchers
let compileTimer: ReturnType<typeof setTimeout> | null = null;

watch([source, options, inputMode], () => {
  if (!compiler.value) return;
  if (compileTimer) clearTimeout(compileTimer);
  compileTimer = setTimeout(compile, 300);
}, { deep: true });

watch(cssOptions, () => {
  if (sfcResult.value?.descriptor?.styles?.length) {
    compile();
  }
}, { deep: true });

// Lifecycle
onMounted(async () => {
  compiler.value = await loadWasm();
  wasmStatus.value = isUsingMock() ? 'mock' : 'ready';
  compile();
});
</script>

<template>
  <div class="app">
    <header class="header">
      <div class="logo">
        <div class="logo-icon">
          <svg viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
            <defs>
              <linearGradient id="gradient" x1="0%" y1="0%" x2="100%" y2="20%">
                <stop offset="0%" stop-color="#E6E9F0"/>
                <stop offset="50%" stop-color="#7B8494"/>
                <stop offset="100%" stop-color="#A34828"/>
              </linearGradient>
              <linearGradient id="gradient-dark" x1="0%" y1="0%" x2="100%" y2="30%">
                <stop offset="0%" stop-color="#B8BDC9"/>
                <stop offset="60%" stop-color="#525A6B"/>
                <stop offset="100%" stop-color="#7D341B"/>
              </linearGradient>
            </defs>
            <g transform="translate(15, 10) skewX(-15)">
              <path d="M 65 0 L 40 60 L 70 20 L 65 0 Z" fill="url(#gradient-dark)" stroke="#3E4654" stroke-width="0.5"/>
              <path d="M 20 0 L 40 60 L 53 13 L 20 0 Z" fill="url(#gradient)" stroke-width="0.5" stroke-opacity="0.6"/>
            </g>
          </svg>
        </div>
        <div class="logo-text">
          <h1>Vize</h1>
          <span class="version">
            Playground
            <span :class="['wasm-status', wasmStatus]">
              {{ wasmStatus === 'loading' ? ' (Loading...)' : wasmStatus === 'mock' ? ' (Mock)' : ' (WASM)' }}
            </span>
          </span>
        </div>
      </div>

      <div class="options">
        <label class="option">
          <span>Preset:</span>
          <select :value="selectedPreset" @change="handlePresetChange(($event.target as HTMLSelectElement).value as PresetKey)">
            <option v-for="(preset, key) in PRESETS" :key="key" :value="key">{{ preset.name }}</option>
          </select>
        </label>

        <label class="option">
          <span>Input:</span>
          <select v-model="inputMode">
            <option value="template">Template</option>
            <option value="sfc">SFC</option>
          </select>
        </label>

        <a href="https://github.com/ubugeeei/vize" target="_blank" rel="noopener noreferrer" class="github-link">
          <svg viewBox="0 0 24 24" width="24" height="24" fill="currentColor">
            <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
          </svg>
        </a>
      </div>
    </header>

    <main class="main">
      <div class="panel input-panel">
        <div class="panel-header">
          <h2>{{ inputMode === 'sfc' ? 'SFC (.vue)' : 'Template' }}</h2>
          <div class="panel-actions">
            <button @click="handlePresetChange(selectedPreset)" class="btn-ghost">Reset</button>
            <button @click="copyToClipboard(source)" class="btn-ghost">Copy</button>
          </div>
        </div>
        <div class="editor-container">
          <MonacoEditor v-model="source" :language="editorLanguage" />
        </div>
      </div>

      <div class="panel output-panel">
        <div class="panel-header">
          <h2>
            Output
            <span v-if="compileTime !== null" class="compile-time">{{ compileTime.toFixed(2) }}ms</span>
          </h2>
          <div class="panel-actions">
            <button @click="copyFullOutput" class="btn-ghost">Copy All Output</button>
          </div>
          <div class="tabs">
            <button :class="['tab', { active: activeTab === 'code' }]" @click="activeTab = 'code'">Code</button>
            <button :class="['tab', { active: activeTab === 'ast' }]" @click="activeTab = 'ast'">AST</button>
            <button :class="['tab', { active: activeTab === 'helpers' }]" @click="activeTab = 'helpers'">Helpers</button>
            <template v-if="inputMode === 'sfc'">
              <button :class="['tab', { active: activeTab === 'sfc' }]" @click="activeTab = 'sfc'">SFC</button>
              <button :class="['tab', { active: activeTab === 'css' }]" @click="activeTab = 'css'">CSS</button>
              <button :class="['tab', { active: activeTab === 'bindings' }]" @click="activeTab = 'bindings'">Bindings</button>
            </template>
          </div>
        </div>

        <div class="output-content">
          <div v-if="isCompiling" class="compiling">
            <div class="spinner" />
            <span>Compiling...</span>
          </div>

          <div v-else-if="error" class="error">
            <h3>Compilation Error</h3>
            <pre>{{ error }}</pre>
          </div>

          <template v-else-if="output">
            <!-- Code Tab -->
            <div v-if="activeTab === 'code'" class="code-output">
              <div class="code-header">
                <h4>Compiled Code</h4>
                <div v-if="isTypeScript" class="code-mode-toggle">
                  <button :class="['toggle-btn', { active: codeViewMode === 'ts' }]" @click="codeViewMode = 'ts'">TS</button>
                  <button :class="['toggle-btn', { active: codeViewMode === 'js' }]" @click="codeViewMode = 'js'">JS</button>
                </div>
              </div>
              <div class="code-actions">
                <button @click="copyToClipboard(isTypeScript && codeViewMode === 'js' ? formattedJsCode : (formattedCode || output.code))" class="btn-ghost">Copy</button>
              </div>
              <div v-if="sfcResult?.bindingMetadata" class="bindings-comment">
                <CodeHighlight :code="'/* Analyzed bindings: ' + JSON.stringify(sfcResult.bindingMetadata, null, 2) + ' */'" language="javascript" />
              </div>
              <CodeHighlight
                v-if="isTypeScript && codeViewMode === 'js'"
                :code="formattedJsCode"
                language="javascript"
                show-line-numbers
              />
              <CodeHighlight
                v-else
                :code="formattedCode || output.code"
                :language="isTypeScript ? 'typescript' : 'javascript'"
                show-line-numbers
              />
            </div>

            <!-- AST Tab -->
            <div v-else-if="activeTab === 'ast'" class="ast-output">
              <h4>Abstract Syntax Tree</h4>
              <CodeHighlight :code="astJson" language="json" show-line-numbers />
            </div>

            <!-- Helpers Tab -->
            <div v-else-if="activeTab === 'helpers'" class="helpers-output">
              <h4>Runtime Helpers Used ({{ output.helpers.length }})</h4>
              <ul v-if="output.helpers.length > 0" class="helpers-list">
                <li v-for="(helper, i) in output.helpers" :key="i" class="helper-item">
                  <span class="helper-name">{{ helper }}</span>
                </li>
              </ul>
              <p v-else class="no-helpers">No runtime helpers needed</p>
            </div>

            <!-- SFC Tab -->
            <div v-else-if="activeTab === 'sfc' && sfcResult" class="sfc-output">
              <h4>SFC Descriptor</h4>

              <div v-if="sfcResult.descriptor.template" class="sfc-block">
                <h5>Template {{ sfcResult.descriptor.template.lang ? `(${sfcResult.descriptor.template.lang})` : '' }}</h5>
                <CodeHighlight :code="sfcResult.descriptor.template.content" language="html" />
              </div>

              <div v-if="sfcResult.descriptor.scriptSetup" class="sfc-block">
                <h5>Script Setup {{ sfcResult.descriptor.scriptSetup.lang ? `(${sfcResult.descriptor.scriptSetup.lang})` : '' }}</h5>
                <CodeHighlight :code="sfcResult.descriptor.scriptSetup.content" language="typescript" />
              </div>

              <div v-if="sfcResult.descriptor.script" class="sfc-block">
                <h5>Script {{ sfcResult.descriptor.script.lang ? `(${sfcResult.descriptor.script.lang})` : '' }}</h5>
                <CodeHighlight :code="sfcResult.descriptor.script.content" language="typescript" />
              </div>

              <div v-if="sfcResult.descriptor.styles.length > 0" class="sfc-block">
                <h5>Styles ({{ sfcResult.descriptor.styles.length }})</h5>
                <div v-for="(style, i) in sfcResult.descriptor.styles" :key="i" class="style-block">
                  <span class="style-meta">
                    <span v-if="style.scoped" class="badge">scoped</span>
                    <span v-if="style.lang" class="badge">{{ style.lang }}</span>
                  </span>
                  <CodeHighlight :code="style.content" language="css" />
                </div>
              </div>
            </div>

            <!-- CSS Tab -->
            <div v-else-if="activeTab === 'css'" class="css-output">
              <h4>CSS Compilation (LightningCSS)</h4>

              <div class="css-options">
                <label class="option checkbox">
                  <input type="checkbox" v-model="cssOptions.minify" />
                  <span>Minify</span>
                </label>
                <label class="option checkbox">
                  <input type="checkbox" v-model="cssOptions.scoped" />
                  <span>Force Scoped</span>
                </label>
              </div>

              <template v-if="cssResult">
                <div class="css-compiled">
                  <h5>Compiled CSS</h5>
                  <div class="code-actions">
                    <button @click="copyToClipboard(formattedCss || cssResult.code)" class="btn-ghost">Copy</button>
                  </div>
                  <CodeHighlight :code="formattedCss || cssResult.code" language="css" show-line-numbers />
                </div>

                <div v-if="cssResult.cssVars.length > 0" class="css-vars">
                  <h5>CSS Variables (v-bind)</h5>
                  <ul class="helpers-list">
                    <li v-for="(v, i) in cssResult.cssVars" :key="i" class="helper-item">
                      <span class="helper-name">{{ v }}</span>
                    </li>
                  </ul>
                </div>

                <div v-if="cssResult.errors.length > 0" class="css-errors">
                  <h5>Errors</h5>
                  <pre v-for="(err, i) in cssResult.errors" :key="i" class="error-message">{{ err }}</pre>
                </div>
              </template>
              <p v-else class="no-css">No styles in this SFC</p>
            </div>

            <!-- Bindings Tab -->
            <div v-else-if="activeTab === 'bindings' && sfcResult?.script?.bindings" class="bindings-output">
              <h4>Script Setup Bindings</h4>
              <table class="bindings-table">
                <thead>
                  <tr>
                    <th>Variable</th>
                    <th>Type</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(bindingType, varName) in sfcResult.script.bindings.bindings" :key="varName">
                    <td class="var-name">{{ varName }}</td>
                    <td class="binding-type"><span class="badge" :class="`badge-${bindingType}`">{{ bindingType }}</span></td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-else-if="activeTab === 'bindings'" class="bindings-output">
              <p class="no-bindings">No bindings information available</p>
            </div>
          </template>
        </div>
      </div>
    </main>

    <footer class="footer">
      <span>Built with Rust + WASM</span>
    </footer>
  </div>
</template>
