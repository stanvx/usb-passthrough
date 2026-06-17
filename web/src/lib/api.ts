import type { Device, ServerStatus, ServerConfig, ConnectRequest, DisconnectRequest, ScanResult } from './types';

const LS_KEY = 'anyplug_api_url';

export function getApiBase(): string {
  if (typeof window === 'undefined') {
    return process.env.NEXT_PUBLIC_API_BASE || '';
  }
  return process.env.NEXT_PUBLIC_API_BASE || localStorage.getItem(LS_KEY) || '';
}

export function setApiBase(url: string): void {
  localStorage.setItem(LS_KEY, url);
}

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const base = getApiBase();
  const res = await fetch(`${base}${path}`, {
    headers: { 'Content-Type': 'application/json', ...options?.headers },
    ...options,
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`API ${res.status}: ${body}`);
  }
  return res.json();
}

export async function getStatus(): Promise<ServerStatus> {
  return apiFetch<ServerStatus>('/api/status');
}

export async function getDevices(): Promise<Device[]> {
  return apiFetch<Device[]>('/api/devices');
}

export async function getConfig(): Promise<ServerConfig> {
  return apiFetch<ServerConfig>('/api/config');
}

export async function updateConfig(config: Partial<ServerConfig>): Promise<ServerConfig> {
  return apiFetch<ServerConfig>('/api/config', {
    method: 'PUT',
    body: JSON.stringify(config),
  });
}

export async function scanServers(): Promise<ScanResult> {
  return apiFetch<ScanResult>('/api/scan', { method: 'POST' });
}

export async function connectDevice(req: ConnectRequest): Promise<void> {
  await apiFetch<void>('/api/connect', {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export async function disconnectDevice(req: DisconnectRequest): Promise<void> {
  await apiFetch<void>('/api/disconnect', {
    method: 'POST',
    body: JSON.stringify(req),
  });
}

export function connectEventsWebSocket(): WebSocket {
  const base = getApiBase();
  const host = base.replace(/^https?:\/\//, '');
  const protocol = base.startsWith('https') ? 'wss:' : 'ws:';
  return new WebSocket(`${protocol}//${host}/api/events`);
}
