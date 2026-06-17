import { describe, it, expect, beforeEach } from 'vitest';
import { getStoredTarget, setStoredTarget } from '@/lib/target';

const LS_TARGET_KEY = 'anyplug_remote_target';

describe('remote target storage', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('returns null when no target is stored', () => {
    expect(getStoredTarget()).toBeNull();
  });

  it('stores and retrieves a target', () => {
    setStoredTarget({ host: '192.168.1.100', port: 3240 });
    expect(getStoredTarget()).toEqual({ host: '192.168.1.100', port: 3240 });
  });

  it('overwrites previous target on new set', () => {
    setStoredTarget({ host: 'old-host', port: 1234 });
    setStoredTarget({ host: 'new-host', port: 5678 });
    expect(getStoredTarget()).toEqual({ host: 'new-host', port: 5678 });
  });

  it('returns null for corrupt JSON', () => {
    localStorage.setItem(LS_TARGET_KEY, 'not-json');
    expect(getStoredTarget()).toBeNull();
  });
});
