import angular from 'angular-eslint';
import tseslint from 'typescript-eslint';
import featureBoundaries from './tools/eslint-rules/feature-boundaries.mjs';
import htmlSafety from './tools/eslint-rules/unsafe-html.mjs';

export default tseslint.config(
  {
    ignores: [
      'dist/**',
      'build/**',
      'coverage/**',
      '.angular/**',
      'node_modules/**',
      '*.min.js',
      '**/generated/**',
    ],
  },
  {
    files: ['apps/**/*.ts', 'libs/**/*.ts'],
    extends: [...tseslint.configs.recommended, ...angular.configs.tsRecommended],
    processor: angular.processInlineTemplates,
    rules: { '@typescript-eslint/no-explicit-any': 'error' },
  },
  {
    // Dashboard rule: component templates live in separate .html files, never inline in TS.
    // (apps/widget and libs/ui are not part of the dashboard — excluded.)
    // styles/animations ceilings are intentionally unrestricted; only `template` is banned.
    files: ['apps/dashboard/**/*.ts'],
    rules: {
      '@angular-eslint/component-max-inline-declarations': [
        'error',
        { template: 0, styles: 1000, animations: 1000 },
      ],
    },
  },
  {
    // Spec test-host components conventionally use tiny inline templates.
    files: ['apps/dashboard/**/*.spec.ts'],
    rules: { '@angular-eslint/component-max-inline-declarations': 'off' },
  },
  {
    files: ['apps/**/*.html', 'libs/**/*.html'],
    extends: [...angular.configs.templateRecommended, ...angular.configs.templateAccessibility],
  },
  {
    files: ['apps/dashboard/src/app/core/**/*.ts'],
    rules: {
      'no-restricted-imports': ['error', { patterns: ['**/features/**', '**/layout/**'] }],
    },
  },
  {
    files: ['apps/dashboard/src/app/shared/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        { patterns: ['**/features/**', '**/layout/**', '**/core/state/**'] },
      ],
    },
  },
  {
    // Module boundaries (MOH-62): features do not import each other; presentational code does not
    // import stores or API services. See tools/eslint-rules/feature-boundaries.mjs.
    files: ['apps/dashboard/src/app/**/*.ts'],
    plugins: { local: featureBoundaries },
    rules: { 'local/feature-boundaries': 'error' },
  },
  {
    // Trusted Types review gate (MOH-170): code that writes HTML, or turns Angular's sanitiser off,
    // needs a disable comment that says why it is safe. Dashboard only: the widget has its own
    // policy. Specs build DOM fixtures freely.
    files: ['apps/dashboard/**/*.ts'],
    ignores: ['**/*.spec.ts'],
    plugins: { 'html-safety': htmlSafety },
    rules: { 'html-safety/unsafe-html': 'error' },
  },
  {
    files: ['apps/dashboard/**/*.html'],
    plugins: { 'html-safety': htmlSafety },
    rules: { 'html-safety/unsafe-html-template': 'error' },
  },
);
