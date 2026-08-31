module.exports = {
  extends: 'expo',
  ignorePatterns: ['/dist/*', '/node_modules/*'],
  rules: {
    // Inline styles are the norm in RN; the design tokens are what enforce
    // consistency here, not a lint rule.
    'react-native/no-inline-styles': 'off',
  },
};
