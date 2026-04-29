/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Disable solid-refresh in test runs: the HMR transform injects a
// `import.meta.url`-derived registration call that jsdom resolves to the
// virtual `/@solid-refresh` URL, which is not a valid file path and crashes
// vitest module loading. `hot: false` skips that transform; tests don't
// need HMR anyway.
export default defineConfig({
  plugins: [solid({ hot: false })],
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/__tests__/**/*.test.ts", "src/**/__tests__/**/*.test.tsx"],
  },
});
