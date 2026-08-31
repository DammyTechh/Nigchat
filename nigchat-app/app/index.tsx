import { Redirect } from 'expo-router';
import React from 'react';

/** The root layout does the real routing; this just picks a destination. */
export default function Index() {
  return <Redirect href="/(tabs)" />;
}
