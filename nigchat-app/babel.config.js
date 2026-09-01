module.exports = function (api) {
  api.cache(true);
  return {
    // babel-preset-expo already wires up the Reanimated / Worklets plugin when
    // the package is installed, so listing it again here double-applies it.
    //
    // If a build ever fails with "Reanimated plugin not found", the SDK 54
    // spelling is `react-native-worklets/plugin` — the worklets runtime was
    // split out of Reanimated in v4. The old
    // `react-native-reanimated/plugin` path no longer exists.
    presets: ['babel-preset-expo'],
  };
};
