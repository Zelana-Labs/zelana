import { Outlet, NavLink } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import {
  LayoutDashboard,
  Users,
  ArrowRightLeft,
  Layers,
  Box,
  Shield,
  ArrowLeftRight,
  Circle,
} from "lucide-react";

const navItems = [
  { path: "/", label: "Dashboard", icon: LayoutDashboard },
  { path: "/accounts", label: "Accounts", icon: Users },
  { path: "/transactions", label: "Transactions", icon: ArrowRightLeft },
  { path: "/batches", label: "Batches", icon: Layers },
  { path: "/blocks", label: "Blocks", icon: Box },
  { path: "/shielded", label: "Shielded", icon: Shield },
  { path: "/bridge", label: "Bridge", icon: ArrowLeftRight },
];

export default function Layout() {
  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: api.getHealth,
    refetchInterval: 3000,
  });

  const { data: stats } = useQuery({
    queryKey: ["stats"],
    queryFn: api.getStats,
  });

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header className="h-14 border-b border-border bg-bg-secondary flex items-center justify-between px-4">
        <div className="flex items-center gap-3">
          <div className="text-accent-green font-bold text-lg tracking-tight">
            ZELANA
          </div>
          <div className="text-text-muted text-sm">Debug Inspector</div>
        </div>

        <div className="flex items-center gap-6 text-sm">
          {/* Batch indicator */}
          {stats && (
            <div className="flex items-center gap-2 px-3 py-1 bg-bg-tertiary rounded border border-border">
              <span className="text-text-secondary">BATCH</span>
              <span className="text-accent-green font-medium">
                {stats.latest_batch_id}
              </span>
            </div>
          )}

          {/* Connection status */}
          <div className="flex items-center gap-4">
            <StatusIndicator
              label="DB"
              connected={health?.dbReader ?? false}
            />
            <StatusIndicator
              label="Sequencer"
              connected={health?.sequencer ?? false}
            />
            <StatusIndicator
              label="Solana"
              connected={health?.solanaRpc ?? false}
            />
          </div>
        </div>
      </header>

      <div className="flex flex-1">
        {/* Sidebar */}
        <aside className="w-48 border-r border-border bg-bg-secondary">
          <nav className="py-2">
            {navItems.map((item) => (
              <NavLink
                key={item.path}
                to={item.path}
                end={item.path === "/"}
                className={({ isActive }) =>
                  `flex items-center gap-3 px-4 py-2.5 text-sm transition-colors ${
                    isActive
                      ? "text-accent-green bg-accent-green/10 border-l-2 border-accent-green"
                      : "text-text-secondary hover:text-text-primary hover:bg-bg-hover"
                  }`
                }
              >
                <item.icon size={16} />
                {item.label}
              </NavLink>
            ))}
          </nav>
        </aside>

        {/* Main content */}
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>

      {/* Status bar */}
      <footer className="h-8 border-t border-border bg-bg-secondary flex items-center justify-between px-4 text-xs text-text-muted">
        <div className="flex items-center gap-4">
          {health && (
            <>
              <span>RPC: {health.solanaRpcUrl}</span>
              <span>Sequencer: {health.sequencerUrl}</span>
            </>
          )}
        </div>
        <div>
          {stats && (
            <span>
              State Root: {stats.latest_state_root.slice(0, 16)}...
            </span>
          )}
        </div>
      </footer>
    </div>
  );
}

function StatusIndicator({
  label,
  connected,
}: {
  label: string;
  connected: boolean;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <Circle
        size={8}
        className={
          connected
            ? "fill-accent-green text-accent-green"
            : "fill-accent-red text-accent-red"
        }
      />
      <span className="text-text-secondary">{label}</span>
    </div>
  );
}
