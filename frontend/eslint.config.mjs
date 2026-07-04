import angular from 'angular-eslint';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/**', '.angular/**', 'node_modules/**'] },
  {
    files: ['apps/**/*.ts', 'libs/**/*.ts'],
    extends: [...tseslint.configs.recommended, ...angular.configs.tsRecommended],
    processor: angular.processInlineTemplates,
    rules: { '@typescript-eslint/no-explicit-any': 'error' },
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
