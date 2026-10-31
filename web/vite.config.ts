import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import wasm from "vite-plugin-wasm";

// The Codama-generated error module (`web/src/generated/zk_pool/src/generated/errors/zkPool.ts`)
// references `process.env.NODE_ENV`. That global does not exist in the browser, so Vite
// must define it at build time.
//
// `@noir-lang/acvm_js` and `@noir-lang/noirc_abi` are wasm-pack-generated modules that
// use `import ... from "*.wasm"`. Vite cannot process them without `vite-plugin-wasm`.
//
// Stage 16.1: `noir_js` is loaded via dynamic `import()` (see
// `web/src/noir/poseidon.ts`, `web/src/noir/hashes.ts`). This means Vite
// can emit it as a separate chunk and the ~3.84 MB WASM is no longer part
// of the initial bundle. The `vite-plugin-top-level-await` plugin was
// removed because the emitted chunk handles TLA natively.
export default defineConfig({
  plugins: [vue(), wasm()],
  define: {
    "process.env.NODE_ENV": JSON.stringify(process.env.NODE_ENV ?? "development"),
  },
  optimizeDeps: {
    exclude: ["@noir-lang/noir_js", "@noir-lang/acvm_js", "@noir-lang/noirc_abi"],
  },
  css: {
    preprocessorOptions: {
      scss: { api: "modern-compiler" },
    },
  },
  server: {
    host: "0.0.0.0",
    port: 5173,
    proxy: { "/api": "http://localhost:4001" },
  },
});
