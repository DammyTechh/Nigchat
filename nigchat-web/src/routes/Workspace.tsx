import { CircleDashed, PhoneOff, WifiOff } from 'lucide-react';
import { useEffect, useState } from 'react';

import { ChatPane } from '../components/ChatPane';
import { ConversationList } from '../components/ConversationList';
import { EmptyState } from '../components/primitives';
import { Rail, type Panel } from '../components/Rail';
import { SettingsPanel } from '../components/SettingsPanel';
import { socket, type SocketStatus } from '../lib/socket';
import { useChats } from '../store/chats';

/**
 * The signed-in workspace.
 *
 * Two panes on desktop, one at a time on mobile. The breakpoint is 768px, and
 * the mobile behaviour is not a shrunken desktop — selecting a conversation
 * replaces the list entirely, with a back button, which is how a phone browser
 * should behave.
 */
export default function Workspace() {
  const [panel, setPanel] = useState<Panel>('chats');
  const [status, setStatus] = useState<SocketStatus>(socket.getStatus());

  const activeId = useChats((state) => state.activeId);
  const load = useChats((state) => state.load);
  const open = useChats((state) => state.open);

  useEffect(() => {
    load().catch(() => {});
    return socket.onStatus(setStatus);
  }, [load]);

  return (
    <div className="flex h-full">
      <Rail active={panel} onChange={setPanel} />

      <div className="flex min-w-0 flex-1 flex-col">
        {/* Connection state is a strip, not a toast: it stays put and explains
            why nothing is arriving. */}
        {status !== 'online' && (
          <div
            role="status"
            className="flex items-center justify-center gap-2 bg-warning/15 px-4 py-1.5 text-caption text-ink-2"
          >
            <WifiOff size={13} />
            {status === 'connecting' ? 'Reconnecting…' : 'Offline — messages will send when you reconnect'}
          </div>
        )}

        <div className="flex min-h-0 flex-1">
          {panel === 'settings' ? (
            <div className="flex-1">
              <SettingsPanel />
            </div>
          ) : panel === 'chats' ? (
            <>
              <aside
                className={
                  // Hidden on mobile once a conversation is open — one pane at
                  // a time on a phone-sized screen.
                  activeId
                    ? 'hidden w-full border-r border-line md:block md:w-[340px] lg:w-[380px]'
                    : 'block w-full border-r border-line md:w-[340px] lg:w-[380px]'
                }
              >
                <ConversationList activeId={activeId} onSelect={(id) => open(id)} />
              </aside>

              <main className={activeId ? 'min-w-0 flex-1' : 'hidden min-w-0 flex-1 md:block'}>
                <ChatPane
                  conversationId={activeId}
                  onBack={() => useChats.setState({ activeId: null })}
                />
              </main>
            </>
          ) : (
            <div className="flex-1">
              <EmptyState
                icon={panel === 'updates' ? CircleDashed : PhoneOff}
                title={panel === 'updates' ? 'Updates' : 'Calls'}
                message={
                  panel === 'updates'
                    ? 'Status posts and channels are available in the mobile app.'
                    : 'Calling from the browser is coming. Use your phone for now.'
                }
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
