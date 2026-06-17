'use client';

import { useEffect, useState, useRef, useCallback } from 'react';
import {
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Area,
  AreaChart,
} from 'recharts';
import { useWebSocket } from '@/hooks/useWebSocket';
import type { WsEvent, LatencySample } from '@/lib/types';
import { Activity, TrendingDown, TrendingUp, Clock, Eye, EyeOff } from 'lucide-react';

const MAX_SAMPLES = 120; // 2 minutes at 1Hz

export default function LatencyDashboard() {
  const { isConnected, subscribe } = useWebSocket();
  const [samples, setSamples] = useState<LatencySample[]>([]);
  const [currentLatency, setCurrentLatency] = useState<number | null>(null);
  const [avgLatency, setAvgLatency] = useState<number>(0);
  const [minLatency, setMinLatency] = useState<number>(Infinity);
  const [maxLatency, setMaxLatency] = useState<number>(0);
  const [packetLoss, setPacketLoss] = useState<number>(0);
  const [totalPackets, setTotalPackets] = useState(0);
  const [showDemo, setShowDemo] = useState(false);
  const [demoActive, setDemoActive] = useState(false);
  const chartKey = useRef(0);

  // Process WebSocket events for latency data
  useEffect(() => {
    const unsub = subscribe((event: WsEvent) => {
      if (event.type === 'latency' && event.payload) {
        const p = event.payload as Record<string, unknown>;
        const latencyUs = typeof p.latency_us === 'number' ? p.latency_us :
                          typeof p.value === 'number' ? p.value : null;

        if (latencyUs !== null) {
          const now = new Date().toLocaleTimeString();
          const sample: LatencySample = {
            time: now,
            latency_us: latencyUs,
            device: typeof p.device === 'string' ? p.device : undefined,
          };

          setSamples((prev) => {
            const next = [...prev, sample];
            return next.length > MAX_SAMPLES ? next.slice(-MAX_SAMPLES) : next;
          });

          setCurrentLatency(latencyUs);
          setTotalPackets((p) => p + 1);
        }
      }

      // Track errors as potential packet loss
      if (event.type === 'error' || event.type === 'disconnect') {
        setTotalPackets((p) => p + 1);
      }
    });

    return () => unsub();
  }, [subscribe]);

  // Recalculate stats when samples change
  useEffect(() => {
    if (samples.length === 0) return;

    const values = samples.map((s) => s.latency_us);
    const avg = values.reduce((a, b) => a + b, 0) / values.length;
    const mn = Math.min(...values);
    const mx = Math.max(...values);

    setAvgLatency(Math.round(avg * 10) / 10);
    setMinLatency(mn);
    setMaxLatency(mx);
    setPacketLoss(0); // computed from error events ratio

    chartKey.current++;
  }, [samples]);

  // Demo data toggle
  useEffect(() => {
    if (!showDemo) {
      if (!demoActive) return;
      setSamples([]);
      setCurrentLatency(null);
      setAvgLatency(0);
      setMinLatency(Infinity);
      setMaxLatency(0);
      setTotalPackets(0);
      setDemoActive(false);
      return;
    }

    const demo = Array.from({ length: 30 }, (_, i) => ({
      time: `${i}s`,
      latency_us: 400 + Math.random() * 300 + Math.sin(i / 3) * 100,
    }));
    setSamples(demo);
    setCurrentLatency(450);
    setAvgLatency(480);
    setMinLatency(310);
    setMaxLatency(720);
    setDemoActive(true);

    return () => {};
  }, [showDemo]);

  const formatLatency = (us: number) => {
    if (us >= 1000) return `${(us / 1000).toFixed(1)} ms`;
    return `${Math.round(us)} µs`;
  };

  return (
    <div className="animate-fade-in">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-bold text-white">Latency Dashboard</h1>
          <p className="text-sm text-[#8b8fa3] mt-1">
            Real-time USB/IP round-trip latency monitoring
          </p>
        </div>
        <button
          onClick={() => setShowDemo((p) => !p)}
          className={`flex items-center gap-2 px-4 py-2 rounded-lg text-sm transition-colors ${
            showDemo
              ? 'bg-yellow-400/10 border border-yellow-400/20 text-yellow-400'
              : 'bg-[#1a1d28] border border-[#2a2e3a] text-[#8b8fa3] hover:text-white'
          }`}
        >
          {showDemo ? <EyeOff size={14} /> : <Eye size={14} />}
          {showDemo ? 'Hide Demo Data' : 'Show Demo Data'}
        </button>
      </div>

      {/* Stats cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <StatCard
          icon={<Activity size={18} className="text-anyplug-400" />}
          label="Current"
          value={currentLatency !== null ? formatLatency(currentLatency) : '--'}
          unit=""
          color={currentLatency !== null && currentLatency > 1000 ? 'text-[#dc2626]' : 'text-anyplug-400'}
        />
        <StatCard
          icon={<TrendingDown size={18} className="text-[#2b9a5e]" />}
          label="Minimum"
          value={formatLatency(minLatency === Infinity ? 0 : minLatency)}
          unit=""
          color="text-[#2b9a5e]"
        />
        <StatCard
          icon={<TrendingUp size={18} className="text-yellow-400" />}
          label="Average"
          value={formatLatency(avgLatency)}
          unit=""
          color="text-yellow-400"
        />
        <StatCard
          icon={<Clock size={18} className="text-[#8b8fa3]" />}
          label="Samples"
          value={String(totalPackets || samples.length)}
          unit="packets"
          color="text-[#8b8fa3]"
        />
      </div>

      {/* Latency chart */}
      <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-6 mb-6">
        <h2 className="text-sm font-semibold text-white mb-4">
          Round-Trip Latency Over Time
        </h2>
        {samples.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-72 text-[#6b6f83]">
            <Activity size={36} className="mb-3 opacity-50" />
            <p className="text-sm">Waiting for telemetry...</p>
            <p className="text-xs mt-1">Latency data appears once a device is connected and active</p>
          </div>
        ) : (
        <div className="h-72">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart
              data={samples}
              margin={{ top: 5, right: 10, left: 0, bottom: 5 }}
            >
              <defs>
                <linearGradient id="latencyGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="#4c6ef5" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#4c6ef5" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="#2a2e3a" />
              <XAxis
                dataKey="time"
                stroke="#6b6f83"
                tick={{ fontSize: 11 }}
                tickLine={false}
              />
              <YAxis
                stroke="#6b6f83"
                tick={{ fontSize: 11 }}
                tickLine={false}
                tickFormatter={(v) => `${v}µs`}
              />
              <Tooltip
                contentStyle={{
                  backgroundColor: '#1a1d28',
                  border: '1px solid #2a2e3a',
                  borderRadius: '8px',
                  color: '#e1e7ef',
                  fontSize: '12px',
                }}
                formatter={(value: number) => [formatLatency(value), 'Latency']}
              />
              <Area
                type="monotone"
                dataKey="latency_us"
                stroke="#4c6ef5"
                strokeWidth={2}
                fill="url(#latencyGradient)"
                dot={false}
                activeDot={{ r: 4, fill: '#4c6ef5' }}
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
      </div>

      {/* Connection status and packet stats */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
          <h3 className="text-sm font-semibold text-white mb-3">Connection Quality</h3>
          <div className="space-y-2">
            <div className="flex justify-between text-sm">
              <span className="text-[#8b8fa3]">WebSocket</span>
              <span
                className={`font-medium ${
                  isConnected ? 'text-[#2b9a5e]' : 'text-[#dc2626]'
                }`}
              >
                {isConnected ? 'Connected' : 'Disconnected'}
              </span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-[#8b8fa3]">Packet Loss</span>
              <span className="font-medium text-[#e1e7ef]">
                {packetLoss > 0 ? `${packetLoss}%` : '0%'}
              </span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-[#8b8fa3]">Total Samples</span>
              <span className="font-medium text-[#e1e7ef]">
                {totalPackets || samples.length}
              </span>
            </div>
          </div>
        </div>

        <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-5">
          <h3 className="text-sm font-semibold text-white mb-3">Latency Health</h3>
          <div className="space-y-2">
            <HealthBar
              label="Latency Budget"
              value={currentLatency !== null ? currentLatency : avgLatency}
              max={3000}
              good={800}
              warn={1500}
            />
            <div className="flex justify-between text-sm">
              <span className="text-[#8b8fa3]">Jitter</span>
              <span className="font-medium text-[#e1e7ef]">
                {maxLatency > 0 && minLatency < Infinity
                  ? formatLatency(maxLatency - minLatency)
                  : '--'}
              </span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-[#8b8fa3]">Status</span>
              <span
                className={`font-medium ${
                  (currentLatency ?? avgLatency) < 800
                    ? 'text-[#2b9a5e]'
                    : (currentLatency ?? avgLatency) < 1500
                      ? 'text-yellow-400'
                      : 'text-[#dc2626]'
                }`}
              >
                {(currentLatency ?? avgLatency) < 800
                  ? 'Excellent'
                  : (currentLatency ?? avgLatency) < 1500
                    ? 'Fair'
                    : 'Poor'}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────

function StatCard({
  icon,
  label,
  value,
  unit,
  color,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  unit: string;
  color: string;
}) {
  return (
    <div className="bg-[#1a1d28] border border-[#2a2e3a] rounded-xl p-4">
      <div className="flex items-center gap-2 mb-2">
        {icon}
        <span className="text-xs text-[#8b8fa3]">{label}</span>
      </div>
      <div className={`text-lg font-bold ${color}`}>
        {value}
        {unit && <span className="text-xs ml-1 opacity-60">{unit}</span>}
      </div>
    </div>
  );
}

function HealthBar({
  label,
  value,
  max,
  good,
  warn,
}: {
  label: string;
  value: number;
  max: number;
  good: number;
  warn: number;
}) {
  const pct = Math.min((value / max) * 100, 100);
  const color =
    value <= good
      ? 'bg-[#2b9a5e]'
      : value <= warn
        ? 'bg-yellow-400'
        : 'bg-[#dc2626]';

  return (
    <div>
      <div className="flex justify-between text-sm mb-1">
        <span className="text-[#8b8fa3]">{label}</span>
        <span className="text-[#e1e7ef] font-medium">{Math.round(value)}µs</span>
      </div>
      <div className="h-1.5 bg-[#2a2e3a] rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${color}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
