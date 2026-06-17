const LS_TARGET_KEY = 'anyplug_remote_target';

export interface RemoteTarget {
  host: string;
  port: number;
}

export function getStoredTarget(): RemoteTarget | null {
  try {
    const raw = localStorage.getItem(LS_TARGET_KEY);
    return raw ? (JSON.parse(raw) as RemoteTarget) : null;
  } catch {
    return null;
  }
}

export function setStoredTarget(target: RemoteTarget): void {
  localStorage.setItem(LS_TARGET_KEY, JSON.stringify(target));
}
