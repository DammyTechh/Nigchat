import { Stack } from 'expo-router';
import React from 'react';

import { useTheme } from '../../src/theme';

export default function AuthLayout() {
  const { theme } = useTheme();
  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: theme.colors.background },
      }}
    />
  );
}
