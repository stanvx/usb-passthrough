import { describe, it, expect, beforeEach, vi } from 'vitest';
import { getApiBase, setApiBase } from '@/lib/api';

const LS_KEY = 'anyplug_api_url';

describe('getApiBase', () => {
  beforeEach(() => {
    localStorage.clear();
    delete process.env.NEXT_PUBLIC_API_BASE;
  });

  it('returns env var when set', () => {
    process.env.NEXT_PUBLIC_API_BASE = 'http://env:3241';
    expect(getApiBase()).toBe('http://env:3241');
  });

  it('falls back to localStorage when no env var', () => {
    localStorage.setItem(LS_KEY, 'http://stored:3241');
    expect(getApiBase()).toBe('http://stored:3241');
  });

  it('returns empty string when nothing is set', () => {
    expect(getApiBase()).toBe('');
  });

  it('prefers env var over localStorage', () => {
    process.env.NEXT_PUBLIC_API_BASE = 'http://env:3241';
    localStorage.setItem(LS_KEY, 'http://stored:3241');
    expect(getApiBase()).toBe('http://env:3241');
  });
});

describe('setApiBase', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('persists to localStorage', () => {
    setApiBase('http://server:3241');
    expect(localStorage.getItem(LS_KEY)).toBe('http://server:3241');
  });
});
