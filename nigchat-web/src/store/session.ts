import { create } from 'zustand';

import { session, setUnauthorizedHandler } from '../lib/api';
import { auth, users } from '../lib/endpoints';
import { socket } from '../lib/socket';
import type { Me } from '../lib/types';

type Status = 'loading' | 'unpaired' | 'paired';

interface SessionState {
  status: Status;
  me: Me | null;
  userId: string | null;
  restore: () => Promise<void>;
  adopt: (pair: {
    access_token: string;
    refresh_token: string;
    user_id: string;
    device_id: string;
  }) => Promise<void>;
  signOut: () => Promise<void>;
}

export const useSession = create<SessionState>((set) => ({
  status: 'loading',
  me: null,
  userId: null,

  async restore() {
    if (!session.access || !session.refresh) {
      set({ status: 'unpaired' });
      return;
    }

    set({ status: 'paired', userId: session.userId });
    socket.connect();

    try {
      set({ me: await users.me() });
    } catch {
      // The interceptor handles a genuine 401; anything else is transient and
      // must not throw the user back to the pairing screen.
    }
  },

  async adopt(pair) {
    session.save(pair);
    set({ status: 'paired', userId: pair.user_id });
    socket.connect();
    try {
      set({ me: await users.me() });
    } catch {
      /* profile fills in on the next load */
    }
  },

  async signOut() {
    await auth.logout().catch(() => {});
    session.clear();
    socket.disconnect();
    set({ status: 'unpaired', me: null, userId: null });
  },
}));

setUnauthorizedHandler(() => {
  // Revoked from the phone, or the refresh token expired. Drop straight to
  // pairing rather than leaving a dead UI on screen.
  session.clear();
  socket.disconnect();
  useSession.setState({ status: 'unpaired', me: null, userId: null });
});
