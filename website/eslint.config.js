import js from "@eslint/js";
import globals from "globals";

export default [
  { ignores: ["node_modules/"] },
  js.configs.recommended,
  {
    files: ["public/**/*.js", "media/**/*.js"],
    languageOptions: { sourceType: "script", globals: globals.browser },
  },
  {
    // Render scripts also hold callbacks that Playwright runs in the page.
    files: ["scripts/**/*.mjs", "eslint.config.js"],
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
  },
];
