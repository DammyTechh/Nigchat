import React from 'react';

import { EmptyState, Header, Screen } from '../../src/components';

/**
 * Calls are not built.
 *
 * This screen previously rendered three hardcoded call records, which made a
 * missing feature look like a working one. Voice and video need an SFU and TURN
 * servers — real infrastructure, not a screen — so until that exists the honest
 * thing is to say so.
 */
export default function CallsScreen() {
  return (
    <Screen edges={['top']}>
      <Header title="Calls" large borderless />
      <EmptyState
        icon="PhoneOff"
        title="Calling is coming"
        message="Voice and video calls are not available yet. Everything else in NigChat works — send a message in the meantime."
      />
    </Screen>
  );
}


