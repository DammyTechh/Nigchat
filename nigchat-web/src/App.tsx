import { useEffect } from 'react';

import { Spinner } from './components/primitives';
import PairScreen from './routes/Pair';
import Workspace from './routes/Workspace';
import { useSession } from './store/session';

export default function App() {
  const status = useSession((state) => state.status);
  const restore = useSession((state) => state.restore);

  useEffect(() => {
    restore();
  }, [restore]);

  if (status === 'loading') {
    return (
      <div className="flex h-full items-center justify-center">
        <Spinner className="h-7 w-7" />
      </div>
    );
  }

  return status === 'paired' ? <Workspace /> : <PairScreen />;
}
