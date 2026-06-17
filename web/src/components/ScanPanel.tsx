'use client';

import { useState, useCallback } from 'react';
import { scanServers } from '@/lib/api';
import type { RemoteDevice } from '@/lib/types';
import { Search, Radio, Loader2, Monitor, Plug, Plus } from 'lucide-react';
import { getStoredTarget, setStoredTarget, type RemoteTarget } from '@/lib/target';

export default function ScanPanel() {
  const [scanning, setScanning] = useState(false);
  const [devices, setDevices] = useState<RemoteDevice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selectedHost, setSelectedHost] = useState('');
  const [selectedPort, setSelectedPort] = useState('3240');
  const [manualHost, setManualHost] = useState('');
  const [selectedTarget, setSelectedTarget] = useState<RemoteTarget | null>(getStoredTarget);

  const handleScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const result = await scanServers();
      setDevices(result.devices);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  }, []);

  function handleSelect(host: string, port: number) {
    const target = { host, port };
    setStoredTarget(target);
    setSelectedTarget(target);
  }

  function handleManualSubmit(e: React.FormEvent) {
    e.preventDefault();
    const host = manualHost.trim();
    if (!host) return;
    const [hostPart, portPart] = host.split(':');
    const port = portPart ? parseInt(portPart, 10) : 3240;
    if (isNaN(port) || port <= 0 || port > 65535) return;
    handleSelect(hostPart, port);
  }

  return (
    <div className="animate-fade-in">
      <div className="mb-6">
        <h1 className="text-xl font-bold text-white">Scan Network</h1>
        <p className="text-sm text-[#8b8fa3] mt-1">
          Discover USB/IP servers on the network or enter one manually
        </p>
      </div>

      {selectedTarget && (
        <div className="mb-6 p-4 rounded-xl bg-[#2b9a5e]/10 border border-[#2b9a5e]/20">
          <div className="flex items-center gap-2 text-sm">
            <Plug size={16} className="text-[#2b9a5e]" />
            <span className="text-white font-medium">
              {selectedTarget.host}:{selectedTarget.port}
            </span>
            <span className="text-[#8b8fa3]">selected for connection</span>
          </div>
        </div>
      )}

      {error && (
        <div className="mb-4 p-4 rounded-lg bg-[#dc2626]/10 border border-[#dc2626]/20 text-sm text-[#dc2626]">
          {error}
        </div>
      )}

      {/* Discover button */}
      <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl mb-6">
        <div className="p-5">
          <h2 className="text-sm font-semibold text-white mb-1">Discover on Network</h2>
          <p className="text-xs text-[#6b6f83] mb-4">
            Find AnyPlug servers broadcasting via mDNS on the local network
          </p>
          <button
            onClick={handleScan}
            disabled={scanning}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-anyplug-600 text-white text-sm font-medium hover:bg-anyplug-500 transition-colors disabled:opacity-50"
          >
            {scanning ? (
              <Loader2 size={16} className="animate-spin" />
            ) : (
              <Search size={16} />
            )}
            {scanning ? 'Scanning...' : 'Scan Network'}
          </button>
        </div>

        {devices.length > 0 && (
          <div className="border-t border-[#2a2e3a] divide-y divide-[#2a2e3a]">
            {devices.map((dev, i) => (
              <div
                key={`${dev.host}-${dev.busid}-${i}`}
                className="flex items-center justify-between px-5 py-4 hover:bg-white/5 transition-colors"
              >
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-anyplug-600/20 flex items-center justify-center">
                    <Monitor size={18} className="text-anyplug-400" />
                  </div>
                  <div>
                    <div className="text-sm text-white font-medium">
                      {dev.host}:{dev.port}
                    </div>
                    <div className="text-xs text-[#8b8fa3] mt-0.5">
                      {dev.path} &middot; VID:{dev.vid.toString(16).padStart(4, '0')}
                      &middot; PID:{dev.pid.toString(16).padStart(4, '0')}
                    </div>
                  </div>
                </div>
                <button
                  onClick={() => handleSelect(dev.host, dev.port)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-anyplug-600/10 border border-anyplug-600/20 text-xs text-anyplug-300 hover:bg-anyplug-600/20 transition-colors"
                >
                  <Plug size={12} />
                  Select
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Manual host entry */}
      <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
        <h2 className="text-sm font-semibold text-white mb-1">Manual Entry</h2>
        <p className="text-xs text-[#6b6f83] mb-4">
          Enter a remote server host and port (e.g. 192.168.1.100:3240)
        </p>
        <form onSubmit={handleManualSubmit} className="flex gap-3">
          <input
            type="text"
            value={manualHost}
            onChange={(e) => setManualHost(e.target.value)}
            placeholder="host:port"
            className="flex-1 px-4 py-2.5 rounded-xl bg-[#0f1117] border border-[#2a2e3a] text-white text-sm focus:outline-none focus:border-anyplug-500/50 transition-colors"
          />
          <button
            type="submit"
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-anyplug-600 text-white text-sm font-medium hover:bg-anyplug-500 transition-colors"
          >
            <Plus size={16} />
            Add
          </button>
        </form>
      </div>
    </div>
  );
}
