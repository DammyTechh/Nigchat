import React, { useState } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';

import {
  Avatar,
  EmptyState,
  Header,
  Icon,
  IconButton,
  Pressable,
  Screen,
  SegmentedControl,
  Text,
} from '../../src/components';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { spacing, useColors } from '../../src/theme';

type CallFilter = 'all' | 'missed';

interface CallEntry {
  id: string;
  name: string;
  kind: 'audio' | 'video';
  direction: 'incoming' | 'outgoing';
  missed: boolean;
  time: string;
}

const CALLS: CallEntry[] = [
  { id: '1', name: 'Ada Obi', kind: 'audio', direction: 'incoming', missed: false, time: 'Today, 09:12' },
  { id: '2', name: 'Design standup', kind: 'video', direction: 'outgoing', missed: false, time: 'Today, 08:40' },
  { id: '3', name: 'Chidi Nwosu', kind: 'audio', direction: 'incoming', missed: true, time: 'Yesterday, 21:05' },
];

export default function CallsScreen() {
  const colors = useColors();
  const [filter, setFilter] = useState<CallFilter>('all');
  const insets = useSafeAreaInsets();
  // Clear the floating glass tab bar.
  const bottomInset = 56 + insets.bottom + spacing.lg;

  const visible = CALLS.filter((call) => (filter === 'missed' ? call.missed : true));

  return (
    <Screen edges={['top']}>
      <Header
        title="Calls"
        large
        borderless
        actions={[{ icon: 'PhoneOutgoing', label: 'New call', onPress: () => {} }]}
      />

      <View style={styles.filters}>
        <SegmentedControl<CallFilter>
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: 'All' },
            { value: 'missed', label: 'Missed' },
          ]}
        />
      </View>

      <FlatList
        data={visible}
        keyExtractor={(item) => item.id}
        contentContainerStyle={visible.length === 0 ? { flex: 1 } : { paddingBottom: bottomInset }}
        ListEmptyComponent={
          <EmptyState
            icon="PhoneOff"
            title="No calls"
            message={
              filter === 'missed'
                ? "You haven't missed any calls."
                : 'Calls you make and receive will show up here.'
            }
          />
        }
        renderItem={({ item }) => (
          <Pressable onPress={() => {}} style={styles.row}>
            <Avatar name={item.name} size="md" />

            <View style={{ flex: 1, gap: 2 }}>
              <Text
                variant="headline"
                tone={item.missed ? 'danger' : 'default'}
                numberOfLines={1}
              >
                {item.name}
              </Text>
              <View style={styles.meta}>
                <Icon
                  name={
                    item.missed
                      ? 'PhoneMissed'
                      : item.direction === 'incoming'
                        ? 'PhoneIncoming'
                        : 'PhoneOutgoing'
                  }
                  size={13}
                  color={item.missed ? colors.danger : colors.textMuted}
                />
                <Text variant="footnote" tone="muted">
                  {item.time}
                </Text>
              </View>
            </View>

            <IconButton
              icon={item.kind === 'video' ? 'Video' : 'Phone'}
              variant="soft"
              size={38}
              onPress={() => {}}
              accessibilityLabel={`Call ${item.name} back`}
            />
          </Pressable>
        )}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  filters: { paddingHorizontal: spacing.base, paddingBottom: spacing.md },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 12,
  },
  meta: { flexDirection: 'row', alignItems: 'center', gap: 5 },
});
