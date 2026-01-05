import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

// Toggle between @vitejs/plugin-vue and vite-plugin-vue-compiler-rs
// Use vue-compiler-rs in both CI and development for self-hosting
// Set VUE_COMPILER_RS_DISABLED=true to fall back to @vitejs/plugin-vue
const USE_VUE_COMPILER_RS = process.env.VUE_COMPILER_RS_DISABLED !== "true";

async function getVuePlugin() {
  if (USE_VUE_COMPILER_RS) {
    try {
      const { vueCompilerRs } =
        await import("../packages/vite-plugin-vue-compiler-rs/dist/index.js");
      console.log(
        "[vite.config] Using vue-compiler-rs for Vue SFC compilation",
      );
      return vueCompilerRs();
    } catch (e) {
      console.warn(
        "[vite.config] Failed to load vue-compiler-rs, falling back to @vitejs/plugin-vue:",
        e,
      );
      return vue();
    }
  }
  console.log("[vite.config] Using @vitejs/plugin-vue for Vue SFC compilation");
  return vue();
}

export default defineConfig(async () => {
  const vuePlugin = await getVuePlugin();

  return {
    base: process.env.CI ? "/vue-compiler-rs/" : "/",
    plugins: [vuePlugin, wasm(), topLevelAwait()],
    server: {
      headers: {
        "Cross-Origin-Opener-Policy": "same-origin",
        "Cross-Origin-Embedder-Policy": "require-corp",
      },
    },
    optimizeDeps: {
      exclude: ["vue-compiler-rs-wasm"],
    },
  };
});
