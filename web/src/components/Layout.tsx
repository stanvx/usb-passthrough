'use client';

import { ReactNode, useState } from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import {
  Monitor,
  Radio,
  Activity,
  Settings,
  Heart,
  Search,
  WifiOff,
  X,
} from 'lucide-react';

interface NavItemProps {
  href: string;
  icon: ReactNode;
  label: string;
  active: boolean;
}

function NavItem({ href, icon, label, active }: NavItemProps) {
  return (
    <Link
      href={href}
      className={`flex items-center gap-3 px-4 py-3 rounded-lg transition-colors ${
        active
          ? 'bg-anyplug-700/30 text-anyplug-300 border-l-2 border-anyplug-500'
          : 'text-[#8b8fa3] hover:bg-white/5 hover:text-white'
      }`}
    >
      <span className="w-5 h-5">{icon}</span>
      <span className="text-sm font-medium">{label}</span>
    </Link>
  );
}

interface LayoutProps {
  children: ReactNode;
  wsConnected: boolean;
}

export default function Layout({ children, wsConnected }: LayoutProps) {
  const pathname = usePathname();
  const [dismissed, setDismissed] = useState(false);
  const showBanner = !wsConnected && !dismissed;

  const navItems = [
    { href: '/devices', icon: <Monitor size={18} />, label: 'Devices' },
    { href: '/scan', icon: <Search size={18} />, label: 'Scan' },
    { href: '/connections', icon: <Radio size={18} />, label: 'Connections' },
    { href: '/latency', icon: <Activity size={18} />, label: 'Latency' },
    { href: '/config', icon: <Settings size={18} />, label: 'Config' },
    { href: '/health', icon: <Heart size={18} />, label: 'Health' },
  ];

  return (
    <div className="flex h-screen overflow-hidden bg-[#0f1117]">
      {/* Sidebar */}
      <aside className="w-56 flex-shrink-0 border-r border-[#2a2e3a] bg-[#1a1d28] flex flex-col">
        <div className="px-4 py-5 border-b border-[#2a2e3a]">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-anyplug-600 flex items-center justify-center">
              <span className="text-white font-bold text-sm">AP</span>
            </div>
            <div>
              <h1 className="text-white font-semibold text-sm">AnyPlug</h1>
              <p className="text-[#6b6f83] text-xs">USB/IP Bridge</p>
            </div>
          </div>
        </div>

        <nav className="flex-1 px-3 py-4 space-y-1">
          {navItems.map((item) => (
            <NavItem
              key={item.href}
              href={item.href}
              icon={item.icon}
              label={item.label}
              active={pathname === item.href}
            />
          ))}
        </nav>

        <div className="px-4 py-3 border-t border-[#2a2e3a]">
          <div className="flex items-center gap-2">
            <span
              className={`w-2 h-2 rounded-full ${
                wsConnected ? 'bg-[#2b9a5e]' : 'bg-[#dc2626]'
              }`}
            />
            <span className="text-xs text-[#6b6f83]">
              {wsConnected ? 'Connected' : 'Disconnected'}
            </span>
          </div>
        </div>
      </aside>

      {/* Main content */}
      <main className="flex-1 overflow-y-auto">
        {showBanner && (
          <div className="flex items-center justify-between px-6 py-2.5 bg-yellow-400/10 border-b border-yellow-400/20">
            <div className="flex items-center gap-2 text-sm text-yellow-400">
              <WifiOff size={14} />
              <span>
                Cannot reach API server |{' '}
                <button
                  onClick={() => {
                    setDismissed(true);
                    window.location.reload();
                  }}
                  className="underline hover:text-yellow-300"
                >
                  Reconfigure
                </button>
              </span>
            </div>
            <button
              onClick={() => setDismissed(true)}
              className="text-yellow-400/60 hover:text-yellow-400"
            >
              <X size={16} />
            </button>
          </div>
        )}
        <div className="p-6">{children}</div>
      </main>
    </div>
  );
}
