import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTypescript from "eslint-config-next/typescript";

export default defineConfig([
  ...nextVitals,
  ...nextTypescript,
  globalIgnores([
    ".next/**",
    ".next-test/**",
    ".next-build/**",
    ".patches/**",
    "test-results/**",
    "playwright-report/**",
    "next-env.d.ts",
  ]),
]);
