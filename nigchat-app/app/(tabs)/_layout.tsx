import { Tabs } from 'expo-router';
import React from 'react';
import { Platform, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Glass, Icon, IconName, Text } from '../../src/components';
import { useChats } from '../../src/store/chats';
import { radius, typography, useColors } from '../../src/theme';

/**
 * Tab bar.
 *
 * Custom rather than the default because of the active indicator: a soft green
 * pill sits behind the selected icon. It reads as a deliberate mark rather than
 * a tinted glyph, and it is the one place the brand colour appears in the app's
 * permanent chrome.
 */
function TabIcon({
  name,
  focused,
  badge,
}: {
  name: IconName;
  focused: boolean;
  badge?: number;
}) {
  const colors = useColors();

  return (
    <View style={styles.iconWrap}>
      <View
        style={[
          styles.pill,
          focused && { backgroundColor: colors.primarySoft },
        ]}
      >
        <Icon
          name={name}
          size={21}
          color={focused ? colors.primary : colors.textMuted}
          strokeWidth={focused ? 2.2 : 1.9}
        />
      </View>

      {badge && badge > 0 ? (
        <View style={[styles.badge, { backgroundColor: colors.primary, borderColor: colors.background }]}>
          <Text
            style={[typography.caption, { color: colors.onPrimary, fontSize: 10 }]}
            allowFontScaling={false}
          >
            {badge > 99 ? '99+' : badge}
          </Text>
        </View>
      ) : null}
    </View>
  );
}

export default function TabsLayout() {
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const unread = useChats((state) => state.totalUnread());

  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: colors.primary,
        tabBarInactiveTintColor: colors.textMuted,
        // Transparent + absolute so content scrolls *under* the glass. Lists
        // add matching bottom padding so nothing is permanently hidden.
        tabBarStyle: {
          position: 'absolute',
          backgroundColor: 'transparent',
          borderTopWidth: 0,
          elevation: 0,
          height: 56 + insets.bottom,
          paddingTop: 6,
          paddingBottom: Math.max(insets.bottom, Platform.OS === 'android' ? 8 : 4),
        },
        tabBarBackground: () => (
          <Glass elevation="chrome" border="top" style={StyleSheet.absoluteFill} />
        ),
        tabBarLabelStyle: { ...typography.caption, marginTop: 2 },
        tabBarHideOnKeyboard: true,
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: 'Chats',
          tabBarIcon: ({ focused }) => (
            <TabIcon name="MessageSquare" focused={focused} badge={unread} />
          ),
        }}
      />
      <Tabs.Screen
        name="updates"
        options={{
          title: 'Updates',
          tabBarIcon: ({ focused }) => <TabIcon name="CircleDashed" focused={focused} />,
        }}
      />
      <Tabs.Screen
        name="calls"
        options={{
          title: 'Calls',
          tabBarIcon: ({ focused }) => <TabIcon name="Phone" focused={focused} />,
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: 'You',
          tabBarIcon: ({ focused }) => <TabIcon name="CircleUser" focused={focused} />,
        }}
      />
    </Tabs>
  );
}

const styles = StyleSheet.create({
  iconWrap: { width: 56, alignItems: 'center' },
  pill: {
    width: 52,
    height: 30,
    borderRadius: radius.pill,
    alignItems: 'center',
    justifyContent: 'center',
  },
  badge: {
    position: 'absolute',
    top: -3,
    right: 8,
    minWidth: 17,
    height: 17,
    borderRadius: 9,
    paddingHorizontal: 4,
    borderWidth: 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
