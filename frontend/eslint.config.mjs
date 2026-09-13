import angular from 'angular-eslint';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/**', 'build/**', 'coverage/**', '.angular/**', 'node_modules/**', '*.min.js'] },
  {
    files: ['apps/**/*.ts', 'libs/**/*.ts'],
    extends: [...tseslint.configs.recommended, ...angular.configs.tsRecommended],
    processor: angular.processInlineTemplates,
    rules: { '@typescript-eslint/no-explicit-any': 'error' },
  },
  {
    // Dashboard rule: component templates live in separate .html files, never inline in TS.
    // (apps/widget and libs/* are frozen scaffolding — excluded.)
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
);
