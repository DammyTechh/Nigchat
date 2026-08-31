import { useRouter } from 'expo-router';
import React from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';

import {
  Avatar,
  Header,
  Icon,
  Pressable,
  Screen,
  Text,
} from '../../src/components';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { spacing, useColors } from '../../src/theme';

/**
 * Updates: ephemeral status posts and followed channels.
 *
 * Structured as a horizontal rail of rings plus a vertical feed of channel
 * cards, rather than one long list of names. Status is a browsing surface, not
 * an inbox, and the layout should say so at a glance.
 */
export default function UpdatesScreen() {
  const colors = useColors();
  const router = useRouter();
  const insets = useSafeAreaInsets();

  const recent = [
    { id: '1', name: 'Ada Obi', time: '12m ago', seen: false },
    { id: '2', name: 'Chidi Nwosu', time: '48m ago', seen: false },
    { id: '3', name: 'Zainab Bello', time: '2h ago', seen: true },
  ];

  const channels = [
    { id: 'c1', name: 'NigChat Product', followers: '12.4K', latest: 'Voice notes now sync across devices', time: '1h' },
    { id: 'c2', name: 'Lagos Tech', followers: '48.1K', latest: 'Meetup this Saturday, Yaba', time: '3h' },
  ];

  return (
    <Screen edges={['top']}>
      <Header
        title="Updates"
        large
        borderless
        actions={[{ icon: 'Search', label: 'Search updates', onPress: () => {} }]}
      />

      <ScrollView showsVerticalScrollIndicator={false} contentContainerStyle={{ paddingBottom: 56 + insets.bottom + spacing.lg }}>
        <View style={styles.railHeader}>
          <Text variant="overline" tone="muted">
            Status
          </Text>
        </View>

        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          contentContainerStyle={styles.rail}
        >
          <Pressable onPress={() => {}} highlight={false} style={styles.railItem}>
            <View>
              <Avatar name="You" size={62} />
              <View style={[styles.addBadge, { backgroundColor: colors.primary, borderColor: colors.background }]}>
                <Icon name="Plus" size={13} color={colors.onPrimary} strokeWidth={2.6} />
              </View>
            </View>
            <Text variant="caption" numberOfLines={1} center style={{ width: 68 }}>
              Add status
            </Text>
          </Pressable>

          {recent.map((person) => (
            <Pressable key={person.id} onPress={() => {}} highlight={false} style={styles.railItem}>
              <Avatar name={person.name} size={62} ring={person.seen ? 'seen' : 'unseen'} />
              <Text variant="caption" numberOfLines={1} center style={{ width: 68 }}>
                {person.name.split(' ')[0]}
              </Text>
            </Pressable>
          ))}
        </ScrollView>

        <View style={styles.sectionHeader}>
          <Text variant="overline" tone="muted">
            Channels
          </Text>
          <Pressable onPress={() => {}} highlight={false}>
            <Text variant="footnote" tone="primary">
              Discover
            </Text>
          </Pressable>
        </View>

        {channels.map((channel) => (
          <Pressable
            key={channel.id}
            onPress={() => router.push(`/chat/${channel.id}`)}
            style={styles.channelRow}
          >
            <Avatar name={channel.name} size="lg" />
            <View style={{ flex: 1, gap: 2 }}>
              <View style={styles.channelTop}>
                <Text variant="headline" numberOfLines={1} style={{ flex: 1 }}>
                  {channel.name}
                </Text>
                <Text variant="caption" tone="muted">
                  {channel.time}
                </Text>
              </View>
              <Text variant="callout" tone="muted" numberOfLines={1}>
                {channel.latest}
              </Text>
              <View style={styles.channelMeta}>
                <Icon name="Users" size={12} color={colors.textMuted} />
                <Text variant="caption" tone="muted">
                  {channel.followers} followers
                </Text>
              </View>
            </View>
          </Pressable>
        ))}
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  railHeader: { paddingHorizontal: spacing.base, paddingBottom: spacing.sm },
  rail: { paddingHorizontal: spacing.base, gap: spacing.base, paddingBottom: spacing.lg },
  railItem: { alignItems: 'center', gap: 6, width: 68 },
  addBadge: {
    position: 'absolute',
    right: 0,
    bottom: 0,
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2.5,
    alignItems: 'center',
    justifyContent: 'center',
  },
  sectionHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: spacing.base,
    paddingTop: spacing.sm,
    paddingBottom: spacing.sm,
  },
  channelRow: {
    flexDirection: 'row',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 12,
    alignItems: 'center',
  },
  channelTop: { flexDirection: 'row', alignItems: 'center', gap: spacing.sm },
  channelMeta: { flexDirection: 'row', alignItems: 'center', gap: 4, marginTop: 1 },
});
