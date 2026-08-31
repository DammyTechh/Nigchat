import { Stack, useRouter, useSegments } from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
import React, { useEffect } from 'react';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import * as Notifications from 'expo-notifications';

import { LockGate } from '../src/components';
import { useAuth } from '../src/store/auth';
import { conversationIdFromNotification, registerForPush } from '../src/utils/push';
import { ThemeProvider, useTheme } from '../src/theme';

// Held until the auth state resolves, so the app never flashes the sign-in
// screen at someone who is already signed in.
SplashScreen.preventAutoHideAsync().catch(() => {});

function RootNavigator() {
  const { theme } = useTheme();
  const status = useAuth((state) => state.status);
  const restore = useAuth((state) => state.restore);
  const segments = useSegments();
  const router = useRouter();

  useEffect(() => {
    restore();
  }, [restore]);

  // Push registration is deliberately after sign-in: asking for the permission
  // on first launch, before the user has any reason to want notifications, is
  // the fastest way to get it denied permanently.
  useEffect(() => {
    if (status !== 'signed-in') return;
    registerForPush();

    const subscription = Notifications.addNotificationResponseReceivedListener((response) => {
      const conversationId = conversationIdFromNotification(response);
      if (conversationId) router.push(`/chat/${conversationId}`);
    });

    return () => subscription.remove();
  }, [status, router]);

  useEffect(() => {
    if (status === 'loading') return;

    SplashScreen.hideAsync().catch(() => {});

    const inAuthFlow = segments[0] === '(auth)';

    if (status === 'signed-out' && !inAuthFlow) {
      router.replace('/(auth)/welcome');
    } else if (status === 'signed-in' && inAuthFlow) {
      router.replace('/(tabs)');
    }
  }, [status, segments, router]);

  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: theme.colors.background },
        // Native push on iOS, fade-through on Android — each platform's own
        // motion language rather than one imposed on both.
        animation: 'slide_from_right',
      }}
    >
      <Stack.Screen name="(auth)" />
      <Stack.Screen name="(tabs)" />
      <Stack.Screen name="chat/[id]" />
      <Stack.Screen
        name="link-device"
        options={{ presentation: 'modal', animation: 'slide_from_bottom' }}
      />
      <Stack.Screen name="new-chat" options={{ presentation: 'modal' }} />
    </Stack>
  );
}

export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <ThemeProvider>
          <LockGate>
            <RootNavigator />
          </LockGate>
        </ThemeProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
