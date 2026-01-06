<script setup lang="ts">
import { ref, computed, watch, onMounted, shallowRef } from 'vue';
import MonacoEditor from './components/MonacoEditor.vue';
import CodeHighlight from './components/CodeHighlight.vue';
import { PRESETS, type PresetKey, type InputMode } from './presets';
import { loadWasm, isWasmLoaded, type CompilerOptions, type CompileResult, type SfcCompileResult, type CssCompileResult, type CssCompileOptions } from './wasm/index';

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

// Computed
const editorLanguage = computed(() => inputMode.value === 'sfc' ? 'vue' : 'html');
const astJson = computed(() => output.value ? JSON.stringify(mapToObject(output.value.ast), null, 2) : '{}');

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
        } else {
          cssResult.value = null;
        }

        if (result?.script?.code) {
          output.value = {
            code: result.script.code,
            preamble: result.template?.preamble || '',
            ast: result.template?.ast || {},
            helpers: result.template?.helpers || [],
          };
        } else if (result?.template) {
          output.value = result.template;
        } else {
          output.value = null;
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
  wasmStatus.value = isWasmLoaded() ? 'ready' : 'mock';
  compile();
});
</script>

<template>
  <div class="app">
    <header class="header">
      <div class="logo">
        <div class="logo-icon">
          <svg viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path d="M16 4L28 8V16C28 22.6274 22.6274 28 16 28C9.37258 28 4 22.6274 4 16V8L16 4Z" stroke="url(#gradient)" stroke-width="2"/>
            <path d="M16 10L22 12V16C22 19.3137 19.3137 22 16 22C12.6863 22 10 19.3137 10 16V12L16 10Z" fill="url(#gradient)"/>
            <defs>
              <linearGradient id="gradient" x1="4" y1="4" x2="28" y2="28">
                <stop stop-color="#42d392"/>
                <stop offset="1" stop-color="#647eff"/>
              </linearGradient>
            </defs>
          </svg>
        </div>
        <div class="logo-text">
          <h1>Vue Compiler RS</h1>
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

        <label class="option checkbox">
          <input type="checkbox" v-model="options.ssr" />
          <span>SSR</span>
        </label>
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
              <h4>Compiled Code</h4>
              <div class="code-actions">
                <button @click="copyToClipboard(output.code)" class="btn-ghost">Copy</button>
              </div>
              <CodeHighlight :code="output.code" language="javascript" show-line-numbers />
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
                    <button @click="copyToClipboard(cssResult.code)" class="btn-ghost">Copy</button>
                  </div>
                  <CodeHighlight :code="cssResult.code" language="css" show-line-numbers />
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
      <a href="https://github.com/ubugeeei/vue-compiler-rs" target="_blank" rel="noopener noreferrer">GitHub</a>
      <span class="divider">|</span>
      <span>Built with Rust + WASM</span>
    </footer>
  </div>
</template>
