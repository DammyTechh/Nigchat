// ESLint 9 flat config. eslint-config-expo 10 ships this format; the old
// .eslintrc.js is no longer read.
const expoConfig = require('eslint-config-expo/flat');

module.exports = [
  ...expoConfig,
  {
    ignores: ['dist/*', 'node_modules/*', '.expo/*'],
  },
  {
    rules: {
      // Inline styles are idiomatic in React Native. Consistency here comes
      // from the design tokens, not from a lint rule.
      'react-native/no-inline-styles': 'off',
    },
  },
];
