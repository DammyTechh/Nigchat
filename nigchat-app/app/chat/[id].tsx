import { useLocalSearchParams, useRouter } from 'expo-router';
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  FlatList,
  KeyboardAvoidingView,
  Platform,
  StyleSheet,
  TextInput,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { socket } from '../../src/api/socket';
import type { Message } from '../../src/api/types';
import {
  Avatar,
  DeliveryState,
  Glass,
  Header,
  IconButton,
  MessageBubble,
  Screen,
  SystemNotice,
  Text,
} from '../../src/components';
import { useAuth } from '../../src/store/auth';
import { useChats } from '../../src/store/chats';
import { radius, spacing, typography, useColors } from '../../src/theme';
import { decodeBase64 } from '../../src/utils/base64';
import { bubbleTime, dayLabel, sameDay, shouldGroup } from '../../src/utils/format';

/** Decodes what the send path encoded. Replaced by the Signal session later. */
function decode(ciphertext: string | null): string {
  if (!ciphertext) return '';
  try {
    return decodeBase64(ciphertext);
  } catch {
    return '';
  }
}

type Row =
  | { kind: 'message'; message: Message; grouped: boolean; tail: boolean }
  | { kind: 'day'; label: string; id: string }
  | { kind: 'notice'; text: string; id: string };

export default function ChatScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const listRef = useRef<FlatList<Row>>(null);

  const myId = useAuth((state) => state.userId);
  const conversation = useChats((state) => state.conversations.find((c) => c.id === id));
  const messages = useChats((state) => state.messages[id!] ?? []);
  const typingUsers = useChats((state) => state.typing[id!] ?? []);
  const { loadMessages, loadOlder, send, markRead, loadConversations } = useChats();
  const conversationsLoaded = useChats((state) => state.loaded);

  const [draft, setDraft] = useState('');
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const typingTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!id) return;
    loadMessages(id).then(() => markRead(id)).catch(() => {});
  }, [id]);

  // Opening straight from a notification on a cold start means the store has
  // never seen this conversation, so the header would read "Chat" and there
  // would be no read marker to compare against.
  useEffect(() => {
    if (!conversationsLoaded) loadConversations().catch(() => {});
  }, [conversationsLoaded, loadConversations]);

  // Reading is a side effect of looking at the screen, so the marker advances
  // whenever new messages arrive while the chat is open.
  useEffect(() => {
    if (id && messages.length) markRead(id).catch(() => {});
  }, [messages.length]);

  const onChangeDraft = useCallback(
    (text: string) => {
      setDraft(text);
      if (!id) return;

      socket.sendTyping(id, 'typing');
      if (typingTimer.current) clearTimeout(typingTimer.current);
      // Stop is sent on a debounce rather than on every keystroke pause, so a
      // fast typist does not emit a frame per character.
      typingTimer.current = setTimeout(() => socket.sendTyping(id, 'stopped'), 2_500);
    },
    [id],
  );

  async function submit() {
    const text = draft.trim();
    if (!text || !id) return;

    setDraft('');
    setReplyTo(null);
    socket.sendTyping(id, 'stopped');

    await send(id, text, replyTo?.id);
    // The list is inverted, so "scroll to newest" is offset 0.
    listRef.current?.scrollToOffset({ offset: 0, animated: true });
  }

  /**
   * Builds the render list once per message change: day separators, run
   * grouping, and the tail flag. Doing this in the row component instead would
   * mean every row reading its neighbours on every frame.
   */
  const rows = useMemo<Row[]>(() => {
    const output: Row[] = [];

    messages.forEach((message, index) => {
      const previous = messages[index - 1];
      const next = messages[index + 1];

      if (!previous || !sameDay(previous.created_at, message.created_at)) {
        output.push({ kind: 'day', label: dayLabel(message.created_at), id: `day-${message.seq}` });
      }

      output.push({
        kind: 'message',
        message,
        grouped: shouldGroup(previous, message),
        tail: !next || !shouldGroup(message, next),
      });
    });

    if (output.length > 0) {
      output.unshift({
        kind: 'notice',
        id: 'e2ee',
        text: 'Messages are end-to-end encrypted. Only you and the people in this chat can read them.',
      });
    }

    // Inverted list: newest first. This is what keeps the view pinned to the
    // bottom when the keyboard opens, without any scroll maths.
    return output.reverse();
  }, [messages]);

  const title = conversation?.title ?? 'Chat';
  const subtitle = typingUsers.length
    ? typingUsers.length === 1
      ? 'typing…'
      : `${typingUsers.length} people typing…`
    : conversation?.kind === 'group'
      ? 'tap for group info'
      : 'tap for contact info';

  return (
    <Screen edges={['top']}>
      <Header
        back
        borderless={false}
        center={
            <View style={styles.headerCenter}>
            <Avatar name={title} size="sm" />
            <View style={{ flex: 1 }}>
              <Text variant="titleSmall" numberOfLines={1}>
                {title}
              </Text>
              <Text
                variant="caption"
                style={{ color: typingUsers.length ? colors.primary : colors.textMuted }}
                numberOfLines={1}
              >
                {subtitle}
              </Text>
            </View>
          </View>
        }
        // Call buttons return when calling exists. A phone icon that does
        // nothing is worse than no phone icon.
        actions={[]}
      />

      <KeyboardAvoidingView
        style={{ flex: 1 }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
        keyboardVerticalOffset={insets.top + 52}
      >
        <FlatList
          ref={listRef}
          data={rows}
          inverted
          keyExtractor={(row) =>
            row.kind === 'message' ? `m-${row.message.seq}` : row.id
          }
          renderItem={({ item }) => {
            if (item.kind === 'day') return <SystemNotice text={item.label} />;
            if (item.kind === 'notice') return <SystemNotice text={item.text} icon="lock" />;

            const { message, grouped, tail } = item;
            const outgoing = message.sender_id === myId || message.sender_id === null;

            if (message.deleted_at) {
              return <SystemNotice text="This message was deleted" />;
            }

            return (
              <MessageBubble
                body={decode(message.ciphertext)}
                time={bubbleTime(message.created_at)}
                outgoing={outgoing}
                grouped={grouped}
                tail={tail}
                edited={!!message.edited_at}
                state={deliveryState(message, conversation?.last_read_seq ?? 0)}
                authorName={
                  conversation?.kind === 'group' && !outgoing ? 'Member' : undefined
                }
              />
            );
          }}
          contentContainerStyle={{ paddingVertical: spacing.md }}
          // Inverted, so "load older" fires at the visual top.
          onEndReached={() => id && loadOlder(id).catch(() => {})}
          onEndReachedThreshold={0.4}
          keyboardDismissMode="interactive"
          showsVerticalScrollIndicator={false}
          maintainVisibleContentPosition={{ minIndexForVisible: 0 }}
        />

        {replyTo ? (
          <View style={[styles.replyBar, { backgroundColor: colors.surfaceRaised, borderTopColor: colors.border }]}>
            <View style={[styles.replyRule, { backgroundColor: colors.primary }]} />
            <View style={{ flex: 1 }}>
              <Text variant="caption" tone="primary">
                Replying to
              </Text>
              <Text variant="footnote" tone="muted" numberOfLines={1}>
                {decode(replyTo.ciphertext)}
              </Text>
            </View>
            <IconButton
              icon="X"
              size={32}
              onPress={() => setReplyTo(null)}
              accessibilityLabel="Cancel reply"
            />
          </View>
        ) : null}

        <Glass
          elevation="panel"
          border="top"
          style={[
            styles.composer,
            { paddingBottom: Math.max(insets.bottom, spacing.sm) },
          ]}
        >
          <View style={[styles.field, { backgroundColor: colors.surfaceRaised, borderColor: colors.border }]}>
            <TextInput
              value={draft}
              onChangeText={onChangeDraft}
              placeholder="Message"
              placeholderTextColor={colors.textMuted}
              style={[typography.body, styles.input, { color: colors.text }]}
              multiline
              maxLength={4096}
            />
          </View>

          {/* The send button only becomes green and solid once there is
              something to send — the affordance appears exactly when it means
              something. */}
          {/* Only appears when there is something to send. Attachments and
              voice notes return once media upload exists. */}
          <IconButton
            icon="ArrowUp"
            variant={draft.trim() ? 'filled' : 'ghost'}
            size={44}
            onPress={submit}
            accessibilityLabel="Send"
          />
        </Glass>
      </KeyboardAvoidingView>
    </Screen>
  );
}

function deliveryState(message: Message, peerReadSeq: number): DeliveryState {
  if (message.metadata?.failed) return 'failed';
  if (message.metadata?.pending || message.seq < 0) return 'pending';
  if (peerReadSeq >= message.seq) return 'read';
  return 'delivered';
}

const styles = StyleSheet.create({
  headerCenter: { flexDirection: 'row', alignItems: 'center', gap: spacing.sm, flex: 1 },
  composer: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    gap: spacing.xs,
    paddingHorizontal: spacing.sm,
    paddingTop: spacing.sm,
  },
  field: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'flex-end',
    borderRadius: radius.xxl,
    borderWidth: StyleSheet.hairlineWidth,
    paddingLeft: spacing.base,
    paddingRight: spacing.xs,
    minHeight: 44,
    maxHeight: 140,
  },
  input: { flex: 1, paddingVertical: Platform.OS === 'ios' ? 11 : 8, maxHeight: 120 },
  replyBar: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: spacing.sm,
    borderTopWidth: StyleSheet.hairlineWidth,
  },
  replyRule: { width: 3, height: 32, borderRadius: 2 },
});
