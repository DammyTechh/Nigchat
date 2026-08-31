import { useFocusEffect, useRouter } from 'expo-router';
import React, { useCallback, useMemo, useState } from 'react';
import { FlatList, RefreshControl, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { socket } from '../../src/api/socket';
import type { ConversationSummary } from '../../src/api/types';
import {
  Avatar,
  Badge,
  Banner,
  EmptyState,
  Header,
  Icon,
  Input,
  Pressable,
  Screen,
  SegmentedControl,
  SkeletonRow,
  Text,
} from '../../src/components';
import { useChats } from '../../src/store/chats';
import { radius, shadow, spacing, useColors } from '../../src/theme';
import { listTimestamp } from '../../src/utils/format';

type Filter = 'all' | 'unread' | 'groups';

export default function ChatsScreen() {
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();

  // The tab bar floats over content, so the last row needs room to clear it.
  const tabBarClearance = 56 + insets.bottom + spacing.lg;

  const conversations = useChats((state) => state.conversations);
  const loading = useChats((state) => state.loading);
  const loaded = useChats((state) => state.loaded);
  const loadConversations = useChats((state) => state.loadConversations);

  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [searching, setSearching] = useState(false);
  const [online, setOnline] = useState(socket.getStatus() === 'online');

  useFocusEffect(
    useCallback(() => {
      loadConversations().catch(() => {});
      return socket.onStatus((status) => setOnline(status === 'online'));
    }, [loadConversations]),
  );

  const visible = useMemo(() => {
    const term = query.trim().toLowerCase();
    return conversations.filter((conversation) => {
      if (conversation.is_archived) return false;
      if (filter === 'unread' && conversation.unread_count === 0) return false;
      if (filter === 'groups' && conversation.kind === 'direct') return false;
      if (term && !(conversation.title ?? '').toLowerCase().includes(term)) return false;
      return true;
    });
  }, [conversations, filter, query]);

  const unreadCount = conversations.filter((c) => c.unread_count > 0).length;

  return (
    <Screen edges={['top']}>
      <Header
        title="Chats"
        large={!searching}
        borderless
        actions={
          searching
            ? []
            : [
                { icon: 'Search', label: 'Search chats', onPress: () => setSearching(true) },
                { icon: 'SquarePen', label: 'New chat', onPress: () => router.push('/new-chat') },
              ]
        }
      />

      {searching ? (
        <View style={styles.searchRow}>
          <Input
            containerStyle={{ flex: 1 }}
            value={query}
            onChangeText={setQuery}
            placeholder="Search chats"
            icon="Search"
            autoFocus
            returnKeyType="search"
            clearButtonMode="while-editing"
          />
          <Pressable
            onPress={() => {
              setSearching(false);
              setQuery('');
            }}
            highlight={false}
            style={{ paddingHorizontal: spacing.sm }}
          >
            <Text variant="body" tone="primary">
              Cancel
            </Text>
          </Pressable>
        </View>
      ) : (
        <View style={styles.filters}>
          <SegmentedControl<Filter>
            value={filter}
            onChange={setFilter}
            options={[
              { value: 'all', label: 'All' },
              { value: 'unread', label: unreadCount ? `Unread ${unreadCount}` : 'Unread' },
              { value: 'groups', label: 'Groups' },
            ]}
          />
        </View>
      )}

      {/* Connection state belongs in the list, not in a toast. A banner that
          stays put tells the user why nothing is arriving. */}
      {!online && loaded ? (
        <Banner tone="warning" icon="WifiOff" text="Connecting… messages will send when you're back online." />
      ) : null}

      {!loaded && loading ? (
        <View>
          {Array.from({ length: 8 }).map((_, index) => (
            <SkeletonRow key={index} />
          ))}
        </View>
      ) : (
        <FlatList
          data={visible}
          keyExtractor={(item) => item.id}
          renderItem={({ item }) => (
            <ConversationRow
              conversation={item}
              onPress={() => router.push(`/chat/${item.id}`)}
            />
          )}
          contentContainerStyle={visible.length === 0 ? { flex: 1 } : { paddingBottom: tabBarClearance }}
          ListEmptyComponent={
            <EmptyState
              icon={query ? 'SearchX' : 'MessageSquarePlus'}
              title={query ? 'No matches' : 'No conversations yet'}
              message={
                query
                  ? `Nothing found for "${query}".`
                  : 'Start a conversation and it will appear here.'
              }
              actionLabel={query ? undefined : 'New chat'}
              onAction={query ? undefined : () => router.push('/new-chat')}
            />
          }
          refreshControl={
            <RefreshControl
              refreshing={loading && loaded}
              onRefresh={loadConversations}
              tintColor={colors.primary}
              colors={[colors.primary]}
            />
          }
          // Long lists of rows benefit far more from these than from
          // memoising the row component.
          initialNumToRender={12}
          maxToRenderPerBatch={10}
          windowSize={9}
          removeClippedSubviews
        />
      )}

      {!searching && (
        <Pressable
          onPress={() => router.push('/new-chat')}
          haptic
          highlight={false}
          accessibilityLabel="New chat"
          style={[
            styles.fab,
            { backgroundColor: colors.primary, bottom: tabBarClearance },
            shadow.raised,
          ]}
        >
          <Icon name="SquarePen" size={22} color={colors.onPrimary} />
        </Pressable>
      )}
    </Screen>
  );
}

function ConversationRow({
  conversation,
  onPress,
}: {
  conversation: ConversationSummary;
  onPress: () => void;
}) {
  const colors = useColors();
  const name = conversation.title ?? 'Unknown';
  const muted = !!conversation.muted_until;
  const unread = conversation.unread_count > 0;

  const preview =
    conversation.last_message_kind && conversation.last_message_kind !== 'text'
      ? attachmentLabel(conversation.last_message_kind)
      : conversation.is_locked
        ? 'Locked chat'
        : 'Tap to open';

  return (
    <Pressable onPress={onPress} style={styles.row}>
      <Avatar name={name} size="lg" />

      <View style={styles.rowBody}>
        <View style={styles.rowTop}>
          <Text variant="headline" numberOfLines={1} style={{ flex: 1 }}>
            {name}
          </Text>
          <Text
            variant="caption"
            // Unread rows put the timestamp in brand green: the eye finds the
            // new conversation without needing to read a single word.
            style={{ color: unread && !muted ? colors.primary : colors.textMuted }}
          >
            {listTimestamp(conversation.last_message_at)}
          </Text>
        </View>

        <View style={styles.rowBottom}>
          {conversation.kind === 'group' ? (
            <Icon name="Users" size={13} color={colors.textMuted} />
          ) : null}
          {conversation.is_locked ? (
            <Icon name="Lock" size={13} color={colors.textMuted} />
          ) : null}

          <Text
            variant="callout"
            tone={unread ? 'secondary' : 'muted'}
            numberOfLines={1}
            style={{ flex: 1 }}
          >
            {preview}
          </Text>

          {conversation.is_pinned ? (
            <Icon name="Pin" size={13} color={colors.textMuted} />
          ) : null}
          {muted ? <Icon name="BellOff" size={13} color={colors.textMuted} /> : null}
          <Badge count={conversation.unread_count} muted={muted} />
        </View>
      </View>
    </Pressable>
  );
}

function attachmentLabel(kind: string) {
  const labels: Record<string, string> = {
    image: 'Photo',
    video: 'Video',
    audio: 'Audio',
    voice_note: 'Voice message',
    document: 'Document',
    sticker: 'Sticker',
    gif: 'GIF',
    location: 'Location',
    contact: 'Contact',
    poll: 'Poll',
    call_event: 'Call',
  };
  return labels[kind] ?? 'Message';
}

const styles = StyleSheet.create({
  searchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.xs,
    paddingHorizontal: spacing.base,
    paddingBottom: spacing.md,
  },
  filters: { paddingHorizontal: spacing.base, paddingBottom: spacing.md },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 11,
  },
  rowBody: { flex: 1, gap: 3 },
  rowTop: { flexDirection: 'row', alignItems: 'center', gap: spacing.sm },
  rowBottom: { flexDirection: 'row', alignItems: 'center', gap: 5 },
  fab: {
    position: 'absolute',
    right: spacing.base,
    bottom: 0,
    width: 56,
    height: 56,
    borderRadius: radius.xl,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
