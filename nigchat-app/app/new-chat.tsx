import { useRouter } from 'expo-router';
import React, { useState } from 'react';
import { FlatList, StyleSheet, View } from 'react-native';

import { conversations as conversationsApi } from '../src/api/endpoints';
import { Avatar, EmptyState, Header, Icon, Input, Pressable, Screen, Text } from '../src/components';
import { spacing, useColors } from '../src/theme';

const ACTIONS = [
  { icon: 'Users' as const, label: 'New group', hint: 'Message many people at once' },
  { icon: 'Megaphone' as const, label: 'New channel', hint: 'Broadcast to followers' },
  { icon: 'UserPlus' as const, label: 'Invite to NigChat', hint: 'Share a link' },
];

export default function NewChatScreen() {
  const router = useRouter();
  const colors = useColors();
  const [query, setQuery] = useState('');

  // Populated by contact sync — numbers are hashed on device before they are
  // sent, so the server never learns the contacts who are not users.
  const contacts: { id: string; name: string; about: string }[] = [];

  const filtered = contacts.filter((contact) =>
    contact.name.toLowerCase().includes(query.trim().toLowerCase()),
  );

  async function openDirect(userId: string) {
    const conversation = await conversationsApi.openDirect(userId);
    router.replace(`/chat/${conversation.id}`);
  }

  return (
    <Screen edges={['top', 'bottom']}>
      <Header title="New chat" back />

      <View style={{ paddingHorizontal: spacing.base, paddingBottom: spacing.md }}>
        <Input
          value={query}
          onChangeText={setQuery}
          placeholder="Search name or number"
          icon="Search"
          autoFocus
          clearButtonMode="while-editing"
        />
      </View>

      <FlatList
        data={filtered}
        keyExtractor={(item) => item.id}
        ListHeaderComponent={
          query ? null : (
            <View style={{ paddingBottom: spacing.sm }}>
              {ACTIONS.map((action) => (
                <Pressable key={action.label} onPress={() => {}} style={styles.action}>
                  <View style={[styles.actionIcon, { backgroundColor: colors.primarySoft }]}>
                    <Icon name={action.icon} size={19} color={colors.primary} />
                  </View>
                  <View style={{ flex: 1 }}>
                    <Text variant="body">{action.label}</Text>
                    <Text variant="footnote" tone="muted">
                      {action.hint}
                    </Text>
                  </View>
                </Pressable>
              ))}
              <Text variant="overline" tone="muted" style={styles.sectionLabel}>
                Contacts on NigChat
              </Text>
            </View>
          )
        }
        renderItem={({ item }) => (
          <Pressable onPress={() => openDirect(item.id)} style={styles.contact}>
            <Avatar name={item.name} size="md" />
            <View style={{ flex: 1 }}>
              <Text variant="headline" numberOfLines={1}>
                {item.name}
              </Text>
              <Text variant="footnote" tone="muted" numberOfLines={1}>
                {item.about}
              </Text>
            </View>
          </Pressable>
        )}
        ListEmptyComponent={
          <EmptyState
            icon="Contact"
            title={query ? 'No matches' : 'No contacts yet'}
            message={
              query
                ? 'Nobody in your contacts matches that.'
                : 'Allow contact access to find people you already know, or invite them to NigChat.'
            }
          />
        }
        contentContainerStyle={filtered.length === 0 && query ? { flex: 1 } : undefined}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  action: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 12,
  },
  actionIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  sectionLabel: { paddingHorizontal: spacing.base, paddingTop: spacing.base },
  contact: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    paddingHorizontal: spacing.base,
    paddingVertical: 11,
  },
});
