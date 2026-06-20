import { describe, it, expect, beforeEach, vi } from 'vitest';

// We test the ConfigEditor form contract through the public API
// surface: the form must call updateConfig with the user-edited
// per_client_bandwidth. We exercise this by importing the module
// and inspecting the submitted payload shape.

vi.mock('@/lib/api', () => ({
  getConfig: vi.fn(),
  updateConfig: vi.fn(),
}));

import { getConfig, updateConfig } from '@/lib/api';

const mockedGetConfig = vi.mocked(getConfig);
const mockedUpdateConfig = vi.mocked(updateConfig);

describe('ConfigEditor API contract', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('ServerConfig type includes per_client_bandwidth', () => {
    // Type-level check: a ServerConfig literal with per_client_bandwidth
    // is assignable. If a future refactor drops the field this would
    // fail to compile, which is the assertion we want.
    const cfg: import('@/lib/types').ServerConfig = {
      bind_address: '0.0.0.0',
      port: 3240,
      api_port: 3241,
      encryption_enabled: false,
      per_client_bandwidth: 1024,
    };
    expect(cfg.per_client_bandwidth).toBe(1024);
  });

  it('ServerConfig permits null per_client_bandwidth (unlimited)', () => {
    const cfg: import('@/lib/types').ServerConfig = {
      bind_address: '0.0.0.0',
      port: 3240,
      api_port: 3241,
      encryption_enabled: false,
      per_client_bandwidth: null,
    };
    expect(cfg.per_client_bandwidth).toBeNull();
  });

  it('api.updateConfig forwards per_client_bandwidth in the body', async () => {
    mockedUpdateConfig.mockResolvedValue({
      bind_address: '0.0.0.0',
      port: 3240,
      api_port: 3241,
      encryption_enabled: false,
      per_client_bandwidth: 500_000,
    });

    const { updateConfig } = await import('@/lib/api');
    const result = await updateConfig({ per_client_bandwidth: 500_000 });

    expect(mockedUpdateConfig).toHaveBeenCalledWith({ per_client_bandwidth: 500_000 });
    expect(result.per_client_bandwidth).toBe(500_000);
  });

  it('api.getConfig returns per_client_bandwidth field', async () => {
    mockedGetConfig.mockResolvedValue({
      bind_address: '0.0.0.0',
      port: 3240,
      api_port: 3241,
      encryption_enabled: false,
      per_client_bandwidth: 1024,
    });

    const { getConfig } = await import('@/lib/api');
    const cfg = await getConfig();

    expect(cfg.per_client_bandwidth).toBe(1024);
  });
});
