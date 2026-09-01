import React from 'react';

import { EmptyState, Header, Screen } from '../../src/components';

/**
 * Status is not built.
 *
 * This screen previously rendered a rail of invented contacts and two invented
 * channels. It looked like a feature. Status needs media upload, a 24-hour
 * reaper and audience rules, none of which exist yet.
 */
export default function UpdatesScreen() {
  return (
    <Screen edges={['top']}>
      <Header title="Updates" large borderless />
      <EmptyState
        icon="CircleDashed"
        title="Status is coming"
        message="Sharing photos and updates that disappear after a day is on the way. Chats work now."
      />
    </Screen>
  );
}
