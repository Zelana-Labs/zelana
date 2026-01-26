import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { formatNumber, truncateHash, timeAgo, getTxTypeColor } from "../lib/formatters";
import {
  Users,
  ArrowRightLeft,
  Layers,
  Box,
  Shield,
  Hash,
  FileText,
  Download,
  Upload,
} from "lucide-react";

export default function Dashboard() {
  const { data: stats, isLoading: statsLoading } = useQuery({
    queryKey: ["stats"],
    queryFn: api.getStats,
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  const { data: transactions } = useQuery({
    queryKey: ["transactions", 0, 10],
    queryFn: () => api.getTransactions(0, 10),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: api.getHealth,
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  if (statsLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-text-secondary">Loading...</div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Stats cards */}
      <div className="grid grid-cols-4 gap-4">
        <StatCard
          icon={Users}
          label="Accounts"
          value={stats?.accounts ?? 0}
          color="green"
        />
        <StatCard
          icon={ArrowRightLeft}
          label="Transactions"
          value={stats?.transactions ?? 0}
          color="blue"
        />
        <StatCard
          icon={Layers}
          label="Batches"
          value={stats?.batches ?? 0}
          color="purple"
        />
        <StatCard
          icon={Box}
          label="Blocks"
          value={stats?.blocks ?? 0}
          color="cyan"
        />
      </div>

      <div className="grid grid-cols-4 gap-4">
        <StatCard
          icon={Hash}
          label="Nullifiers"
          value={stats?.nullifiers ?? 0}
          color="yellow"
        />
        <StatCard
          icon={Shield}
          label="Commitments"
          value={stats?.commitments ?? 0}
          color="purple"
        />
        <StatCard
          icon={Download}
          label="Deposits"
          value={stats?.deposits ?? 0}
          color="green"
        />
        <StatCard
          icon={Upload}
          label="Withdrawals"
          value={stats?.withdrawals ?? 0}
          color="yellow"
        />
      </div>

      <div className="grid grid-cols-3 gap-6">
        {/* Recent Transactions */}
        <div className="col-span-2 card">
          <div className="card-header">
            <h2 className="font-medium">Recent Transactions</h2>
            <span className="text-text-muted text-sm">
              {transactions?.total ?? 0} total
            </span>
          </div>
          <div className="overflow-x-auto">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Hash</th>
                  <th>Type</th>
                  <th>Status</th>
                  <th>Time</th>
                </tr>
              </thead>
              <tbody>
                {transactions?.items.map((tx) => (
                  <tr key={tx.tx_hash}>
                    <td className="font-mono text-xs">
                      {truncateHash(tx.tx_hash)}
                    </td>
                    <td>
                      <TxTypeBadge type={tx.tx_type} />
                    </td>
                    <td>
                      <StatusBadge status={tx.status} />
                    </td>
                    <td className="text-text-secondary text-xs">
                      {timeAgo(tx.received_at)}
                    </td>
                  </tr>
                ))}
                {(!transactions || transactions.items.length === 0) && (
                  <tr>
                    <td colSpan={4} className="text-center text-text-muted py-8">
                      No transactions yet
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        {/* Connection Status */}
        <div className="card">
          <div className="card-header">
            <h2 className="font-medium">Connection Status</h2>
          </div>
          <div className="card-body space-y-4">
            <ConnectionRow
              label="RPC URL"
              value={health?.solanaRpcUrl ?? ""}
              connected={health?.solanaRpc ?? false}
            />
            <ConnectionRow
              label="WS URL"
              value={health?.solanaRpcUrl?.replace("http", "ws").replace("8899", "8900") ?? ""}
              connected={health?.solanaRpc ?? false}
            />
            <ConnectionRow
              label="Sequencer"
              value={health?.sequencerUrl ?? ""}
              connected={health?.sequencer ?? false}
            />
            <ConnectionRow
              label="DB Reader"
              value="127.0.0.1:3457"
              connected={health?.dbReader ?? false}
            />

            <div className="pt-4 border-t border-border">
              <div className="text-text-muted text-xs mb-2">Latest State Root</div>
              <div className="font-mono text-xs break-all text-text-secondary">
                {stats?.latest_state_root ?? "—"}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function StatCard({
  icon: Icon,
  label,
  value,
  color,
}: {
  icon: React.ElementType;
  label: string;
  value: number;
  color: "green" | "blue" | "purple" | "yellow" | "cyan";
}) {
  const colorClasses = {
    green: "text-accent-green bg-accent-green/10",
    blue: "text-accent-blue bg-accent-blue/10",
    purple: "text-accent-purple bg-accent-purple/10",
    yellow: "text-accent-yellow bg-accent-yellow/10",
    cyan: "text-accent-cyan bg-accent-cyan/10",
  };

  return (
    <div className="card p-4">
      <div className="flex items-center gap-3">
        <div className={`p-2 rounded ${colorClasses[color]}`}>
          <Icon size={18} />
        </div>
        <div>
          <div className="text-2xl font-semibold">{formatNumber(value)}</div>
          <div className="text-text-muted text-sm">{label}</div>
        </div>
      </div>
    </div>
  );
}

function TxTypeBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    deposit: "badge-success",
    transfer: "badge-info",
    shielded: "badge-purple",
    withdrawal: "badge-warning",
  };

  return (
    <span className={`badge ${colors[type] || "badge-info"}`}>
      {type.toUpperCase()}
    </span>
  );
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: "badge-warning",
    included: "badge-info",
    executed: "badge-success",
    settled: "badge-success",
    failed: "badge-error",
  };

  return (
    <span className={`badge ${colors[status] || "badge-info"}`}>
      {status.toUpperCase()}
    </span>
  );
}

function ConnectionRow({
  label,
  value,
  connected,
}: {
  label: string;
  value: string;
  connected: boolean;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-2">
        <div
          className={`w-2 h-2 rounded-full ${connected ? "bg-accent-green" : "bg-accent-red"
            }`}
        />
        <span className="text-text-secondary text-sm">{label}</span>
      </div>
      <span className="text-text-muted text-xs font-mono">{value}</span>
    </div>
  );
}
