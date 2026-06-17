'use client';

import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react';
import { getApiBase, setApiBase } from '@/lib/api';

interface ServerUrlContextValue {
  serverUrl: string;
  isFirstLaunch: boolean;
  setServerUrl: (url: string) => void;
}

const ServerUrlContext = createContext<ServerUrlContextValue | null>(null);

export function ServerUrlProvider({ children }: { children: ReactNode }) {
  const [serverUrl, setServerUrlState] = useState('');
  const [isFirstLaunch, setIsFirstLaunch] = useState(false);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const saved = getApiBase();
    setServerUrlState(saved);
    if (!saved) {
      setIsFirstLaunch(true);
    }
    setReady(true);
  }, []);

  const setServerUrl = useCallback((url: string) => {
    setApiBase(url);
    setServerUrlState(url);
    setIsFirstLaunch(false);
  }, []);

  return (
    <ServerUrlContext.Provider value={{ serverUrl, isFirstLaunch, setServerUrl }}>
      {ready ? children : null}
    </ServerUrlContext.Provider>
  );
}

export function useServerUrl(): ServerUrlContextValue {
  const ctx = useContext(ServerUrlContext);
  if (!ctx) throw new Error('useServerUrl must be used within ServerUrlProvider');
  return ctx;
}
