'use client';

import { useState, useCallback, useEffect } from 'react';
import { getDevices, connectDevice, disconnectDevice } from '@/lib/api';
import type { Device, ConnectionEvent } from '@/lib/types';
import { getStoredTarget } from '@/lib/target';
import {
  Plug,
  PlugZap,
  X,
  Loader2,
  CheckCircle2,
  AlertCircle,
  Radio,
} from 'lucide-react';

interface ConnectionManagerProps {
  onEvent?: (event: ConnectionEvent) => void;
}

export default function ConnectionManager({ onEvent }: ConnectionManagerProps) {
  const [devices, setDevices] = useState<Device[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionBusId, setActionBusId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const target = getStoredTarget();

  const fetchDevices = useCallback(async () => {
    try {
      const data = await getDevices();
      setDevices(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDevices();
    const interval = setInterval(fetchDevices, 5000);
    return () => clearInterval(interval);
  }, [fetchDevices]);

  async function handleConnect(busid: string) {
    setActionBusId(busid);
    setError(null);
    try {
      const host = target?.host || 'localhost';
      const port = target?.port || 3240;
      await connectDevice({ host, port, busid });
      await fetchDevices();
      onEvent?.({
        busid,
        device_name: busid,
        status: 'connected',
        timestamp: new Date().toISOString(),
      });
    } catch (e) {
      setError(String(e));
      onEvent?.({
        busid,
        device_name: busid,
        status: 'error',
        timestamp: new Date().toISOString(),
      });
    } finally {
      setActionBusId(null);
    }
  }

  async function handleDisconnect(busid: string) {
    setActionBusId(busid);
    setError(null);
    try {
      await disconnectDevice({ busid });
      await fetchDevices();
      onEvent?.({
        busid,
        device_name: busid,
        status: 'disconnected',
        timestamp: new Date().toISOString(),
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setActionBusId(null);
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin w-8 h-8 border-2 border-anyplug-500 border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="animate-fade-in">
      <div className="mb-6">
        <h1 className="text-xl font-bold text-white">Connection Management</h1>
        <p className="text-sm text-[#8b8fa3] mt-1">
          Discover available devices and manage connections
        </p>
      </div>

      {error && (
        <div className="mb-4 p-4 rounded-lg bg-[#dc2626]/10 border border-[#dc2626]/20 text-sm text-[#dc2626]">
          {error}
        </div>
      )}

      {target && (
        <div className="mb-4 p-3 rounded-lg bg-anyplug-600/10 border border-anyplug-600/20 text-sm">
          <span className="text-[#8b8fa3]">Remote server: </span>
          <span className="text-anyplug-300 font-medium">{target.host}:{target.port}</span>
          <span className="text-[#6b6f83] ml-2 text-xs">(change in Scan)</span>
        </div>
      )}

      <div className="grid gap-3">
        {devices.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-48 text-[#6b6f83] rounded-xl bg-[#1a1d28] border border-[#2a2e3a]">
            <Radio size={36} className="mb-3 opacity-50" />
            <p className="text-sm">No devices available</p>
          </div>
        ) : (
          devices.map((device) => {
            const isConnected = device.status === 'exported';
            const isAction = actionBusId === device.busid;

            return (
              <div
                key={device.busid}
                className={`bg-[#1a1d28] border rounded-xl p-4 transition-colors ${
                  isConnected
                    ? 'border-anyplug-600/40'
                    : 'border-[#2a2e3a] hover:border-[#3a3e4a]'
                }`}
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div
                      className={`w-10 h-10 rounded-lg flex items-center justify-center ${
                        isConnected
                          ? 'bg-[#2b9a5e]/20'
                          : 'bg-[#6b6f83]/10'
                      }`}
                    >
                      {isConnected ? (
                        <PlugZap size={20} className="text-[#2b9a5e]" />
                      ) : (
                        <Plug size={20} className="text-[#8b8fa3]" />
                      )}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-white text-sm font-medium">
                          {device.path}
                        </span>
                        {isConnected && (
                          <span className="flex items-center gap-1 text-xs text-[#2b9a5e]">
                            <CheckCircle2 size={12} />
                            Active
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-[#8b8fa3] mt-0.5">
                        {device.busid} &middot; VID:{device.vid.toString(16).padStart(4, '0')}
                        &middot; PID:{device.pid.toString(16).padStart(4, '0')}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3">
                    {isConnected ? (
                      <button
                        onClick={() => handleDisconnect(device.busid)}
                        disabled={isAction}
                        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[#dc2626]/10 border border-[#dc2626]/20 text-sm text-[#dc2626] hover:bg-[#dc2626]/20 transition-colors disabled:opacity-50"
                      >
                        {isAction ? (
                          <Loader2 size={14} className="animate-spin" />
                        ) : (
                          <X size={14} />
                        )}
                        Disconnect
                      </button>
                    ) : (
                      <button
                        onClick={() => handleConnect(device.busid)}
                        disabled={isAction}
                        className="flex items-center gap-2 px-4 py-2 rounded-lg bg-anyplug-600/10 border border-anyplug-600/20 text-sm text-anyplug-300 hover:bg-anyplug-600/20 transition-colors disabled:opacity-50"
                      >
                        {isAction ? (
                          <Loader2 size={14} className="animate-spin" />
                        ) : (
                          <PlugZap size={14} />
                        )}
                        Connect
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
