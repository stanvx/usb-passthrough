export interface Device {
  busid: string;
  vid: number;
  pid: number;
  speed: number;
  status: string;
  connected_client: string | null;
  path: string;
}

export interface ServerStatus {
  status: string;
  version: string;
  uptime: number;
  active_connections: number;
  devices_count: number;
  uptime_secs: number;
  memory_usage: number;
  error_count: number;
}

export interface ServerConfig {
  bind_address: string;
  port: number;
  api_port: number;
  encryption_enabled: boolean;
  per_client_bandwidth: number | null;
}

export interface ConnectRequest {
  host: string;
  port: number;
  busid: string;
}

export interface DisconnectRequest {
  busid: string;
}

export interface RemoteDevice {
  host: string;
  port: number;
  busid: string;
  vid: number;
  pid: number;
  path: string;
}

export interface ScanResult {
  devices: RemoteDevice[];
}

export interface ApiError {
  error: string;
  correlation_id?: string;
}

export interface WsEvent {
  type: string;
  payload: unknown;
  timestamp: string;
}

export interface LatencySample {
  time: string;
  latency_us: number;
  device?: string;
}

export interface ConnectionEvent {
  busid: string;
  device_name: string;
  status: 'connected' | 'disconnected' | 'connecting' | 'error';
  latency_us?: number;
  timestamp: string;
}
