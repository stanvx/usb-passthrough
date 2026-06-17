'use client';

import { ReactNode } from 'react';
import './globals.css';
import AppLayout from '@/components/Layout';
import FirstLaunchDialog from '@/components/FirstLaunchDialog';
import { useWebSocket } from '@/hooks/useWebSocket';
import { ServerUrlProvider, useServerUrl } from '@/contexts/ServerUrlContext';

function LayoutInner({ children }: { children: ReactNode }) {
  const { isConnected } = useWebSocket();
  const { isFirstLaunch } = useServerUrl();

  if (isFirstLaunch) {
    return (
      <html lang="en">
        <body>
          <FirstLaunchDialog />
        </body>
      </html>
    );
  }

  return (
    <html lang="en">
      <body>
        <AppLayout wsConnected={isConnected}>{children}</AppLayout>
      </body>
    </html>
  );
}

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <ServerUrlProvider>
      <LayoutInner>{children}</LayoutInner>
    </ServerUrlProvider>
  );
}
