'use client';

import { useEffect, useState, useCallback } from 'react';
import { getConfig, updateConfig } from '@/lib/api';
import type { ServerConfig } from '@/lib/types';
import { Settings, Save, RefreshCw } from 'lucide-react';

interface PortInfo {
  key: string;
  label: string;
  value: number;
  default: number;
  description: string;
}

const PORT_FIELDS: PortInfo[] = [
  { key: 'wire', label: 'Wire Port', value: 3240, default: 3240, description: 'USB/IP protocol traffic — the core passthrough port for USB data transfer' },
  { key: 'api', label: 'API Port', value: 3241, default: 3241, description: 'REST API and WebSocket — web console, health checks, configuration' },
  { key: 'mdns', label: 'mDNS Port', value: 5353, default: 5353, description: 'Service discovery — broadcasts server presence on the local network' },
];

export default function ConfigEditor() {
  const [config, setConfig] = useState<ServerConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const fetchConfig = useCallback(async () => {
    try {
      setError(null);
      const data = await getConfig();
      setConfig(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchConfig();
  }, [fetchConfig]);

  function handleBindChange(value: string) {
    if (!config) return;
    setConfig({ ...config, bind_address: value });
  }

  function handlePortChange(field: PortInfo, value: string) {
    if (!config) return;
    const port = parseInt(value, 10);
    if (!isNaN(port) && port > 0 && port <= 65535) {
      setConfig({ ...config, [field.key === 'wire' ? 'port' : field.key === 'api' ? 'api_port' : 'mdns_port']: port });
    }
  }

  function handleEncryptionToggle() {
    if (!config) return;
    setConfig({ ...config, encryption_enabled: !config.encryption_enabled });
  }

  async function handleSave() {
    if (!config) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const result = await updateConfig(config);
      setConfig(result);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
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
    <div className="animate-fade-in max-w-2xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-bold text-white">Configuration</h1>
          <p className="text-sm text-[#8b8fa3] mt-1">
            View and modify server settings
          </p>
        </div>
        <button
          onClick={fetchConfig}
          className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[#1a1d28] border border-[#2a2e3a] text-sm text-[#8b8fa3] hover:text-white transition-colors"
        >
          <RefreshCw size={14} />
          Reload
        </button>
      </div>

      {error && (
        <div className="mb-4 p-4 rounded-lg bg-[#dc2626]/10 border border-[#dc2626]/20 text-sm text-[#dc2626]">
          {error}
        </div>
      )}

      {saved && (
        <div className="mb-4 p-4 rounded-lg bg-[#2b9a5e]/10 border border-[#2b9a5e]/20 text-sm text-[#2b9a5e]">
          Configuration saved successfully
        </div>
      )}

      {config && (
        <div className="space-y-6">
          {/* Bind Address */}
          <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
            <label className="block text-sm font-medium text-white mb-2">
              Bind Address
            </label>
            <input
              type="text"
              value={config.bind_address}
              onChange={(e) => handleBindChange(e.target.value)}
              className="w-full px-4 py-2.5 rounded-lg bg-[#0f1117] border border-[#2a2e3a] text-white text-sm focus:outline-none focus:border-anyplug-500/50 transition-colors"
            />
            <p className="text-xs text-[#6b6f83] mt-1.5">
              IP address the server binds to for API access
            </p>
          </div>

          {/* Ports */}
          {PORT_FIELDS.map((field) => (
            <div key={field.key} className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
              <label className="block text-sm font-medium text-white mb-2">
                {field.label}
              </label>
              <input
                type="number"
                value={field.value}
                onChange={(e) => handlePortChange(field, e.target.value)}
                min={1}
                max={65535}
                className="w-full px-4 py-2.5 rounded-lg bg-[#0f1117] border border-[#2a2e3a] text-white text-sm focus:outline-none focus:border-anyplug-500/50 transition-colors"
              />
              <p className="text-xs text-[#6b6f83] mt-1.5">
                {field.description}
              </p>
            </div>
          ))}

          {/* Encryption Toggle */}
          <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
            <div className="flex items-center justify-between">
              <div>
                <label className="text-sm font-medium text-white">
                  Encryption
                </label>
                <p className="text-xs text-[#6b6f83] mt-0.5">
                  AES-256-GCM tunnel for USB/IP traffic
                </p>
              </div>
              <button
                onClick={handleEncryptionToggle}
                className={`relative w-12 h-6 rounded-full transition-colors ${
                  config.encryption_enabled
                    ? 'bg-anyplug-600'
                    : 'bg-[#2a2e3a]'
                }`}
              >
                <span
                  className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white transition-transform ${
                    config.encryption_enabled
                      ? 'translate-x-6'
                      : 'translate-x-0'
                  }`}
                />
              </button>
            </div>
          </div>

          {/* Save button */}
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-2 px-6 py-3 rounded-lg bg-anyplug-600 text-white text-sm font-medium hover:bg-anyplug-500 transition-colors disabled:opacity-50"
          >
            {saving ? (
              <RefreshCw size={16} className="animate-spin" />
            ) : (
              <Save size={16} />
            )}
            {saving ? 'Saving...' : 'Save Configuration'}
          </button>
        </div>
      )}
    </div>
  );
}
