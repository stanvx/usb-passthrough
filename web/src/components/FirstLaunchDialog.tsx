'use client';

import { useState } from 'react';
import { useServerUrl } from '@/contexts/ServerUrlContext';
import { Server } from 'lucide-react';

export default function FirstLaunchDialog() {
  const { serverUrl, setServerUrl } = useServerUrl();
  const [input, setInput] = useState(() => {
    if (serverUrl) return serverUrl;
    const host = typeof window !== 'undefined' ? window.location.hostname : 'localhost';
    return `http://${host}:3241`;
  });
  const [error, setError] = useState('');

  function handleSubmit() {
    let url = input.trim();
    if (!url) {
      setError('Please enter a server URL');
      return;
    }
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = `http://${url}`;
    }
    try {
      new URL(url);
    } catch {
      setError('Invalid URL');
      return;
    }
    setError('');
    setServerUrl(url);
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-2xl p-8 w-full max-w-md mx-4 shadow-2xl">
        <div className="flex items-center gap-3 mb-6">
          <div className="w-10 h-10 rounded-xl bg-anyplug-600/20 flex items-center justify-center">
            <Server size={22} className="text-anyplug-400" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-white">Connect to Server</h2>
            <p className="text-sm text-[#8b8fa3]">Enter the AnyPlug server URL</p>
          </div>
        </div>

        <input
          type="text"
          value={input}
          onChange={(e) => { setInput(e.target.value); setError(''); }}
          onKeyDown={(e) => e.key === 'Enter' && handleSubmit()}
          placeholder="http://localhost:3241"
          className="w-full px-4 py-3 rounded-xl bg-[#0f1117] border border-[#2a2e3a] text-white text-sm focus:outline-none focus:border-anyplug-500/50 transition-colors"
          autoFocus
        />

        {error && (
          <p className="mt-2 text-xs text-[#dc2626]">{error}</p>
        )}

        <p className="mt-3 text-xs text-[#6b6f83]">
          This is the address of the AnyPlug server&apos;s REST API (port 3241 by default).
          The web console needs this to communicate with the server.
        </p>

        <button
          onClick={handleSubmit}
          className="mt-6 w-full px-6 py-3 rounded-xl bg-anyplug-600 text-white text-sm font-medium hover:bg-anyplug-500 transition-colors"
        >
          Connect
        </button>
      </div>
    </div>
  );
}
