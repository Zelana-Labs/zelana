import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, Transaction } from "../lib/api";
import {
  truncateHash,
  timeAgo,
  formatBalance,
  copyToClipboard,
} from "../lib/formatters";
import { Copy, Check, Filter } from "lucide-react";

const TX_TYPES = ["all", "deposit", "transfer", "shielded", "withdrawal"];
const STATUSES = ["all", "pending", "included", "executed", "settled", "failed"];

export default function Transactions() {
  const [page, setPage] = useState(0);
  const [typeFilter, setTypeFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState("all");
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["transactions", page, limit, typeFilter, statusFilter],
    queryFn: () =>
      api.getTransactions(page * limit, limit, {
        tx_type: typeFilter !== "all" ? typeFilter : undefined,
        status: statusFilter !== "all" ? statusFilter : undefined,
      }),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Transactions</h1>

        <div className="flex items-center gap-3">
          <Filter size={16} className="text-text-muted" />

          <select
            value={typeFilter}
            onChange={(e) => {
              setTypeFilter(e.target.value);
              setPage(0);
            }}
            className="px-3 py-1.5 bg-bg-tertiary border border-border rounded text-sm focus:outline-none focus:border-accent-green"
          >
            {TX_TYPES.map((type) => (
              <option key={type} value={type}>
                {type === "all" ? "All Types" : type.charAt(0).toUpperCase() + type.slice(1)}
              </option>
            ))}
          </select>

          <select
            value={statusFilter}
            onChange={(e) => {
              setStatusFilter(e.target.value);
              setPage(0);
            }}
            className="px-3 py-1.5 bg-bg-tertiary border border-border rounded text-sm focus:outline-none focus:border-accent-green"
          >
            {STATUSES.map((status) => (
              <option key={status} value={status}>
                {status === "all" ? "All Status" : status.charAt(0).toUpperCase() + status.slice(1)}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="card">
        <table className="data-table">
          <thead>
            <tr>
              <th>Hash</th>
              <th>Type</th>
              <th>Status</th>
              <th>Batch</th>
              <th className="text-right">Amount</th>
              <th>Time</th>
              <th className="w-16"></th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={7} className="text-center py-8 text-text-muted">
                  Loading...
                </td>
              </tr>
            ) : data && data.items.length > 0 ? (
              data.items.map((tx) => <TxRow key={tx.tx_hash} tx={tx} />)
            ) : (
              <tr>
                <td colSpan={7} className="text-center py-8 text-text-muted">
                  No transactions found
                </td>
              </tr>
            )}
          </tbody>
        </table>

        {/* Pagination */}
        {data && data.total > limit && (
          <div className="px-4 py-3 border-t border-border flex items-center justify-between">
            <div className="text-sm text-text-muted">
              Showing {page * limit + 1} -{" "}
              {Math.min((page + 1) * limit, data.total)} of {data.total}
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                disabled={page === 0}
                className="px-3 py-1 text-sm bg-bg-tertiary border border-border rounded hover:bg-bg-hover disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Previous
              </button>
              <button
                onClick={() => setPage((p) => p + 1)}
                disabled={(page + 1) * limit >= data.total}
                className="px-3 py-1 text-sm bg-bg-tertiary border border-border rounded hover:bg-bg-hover disabled:opacity-50 disabled:cursor-not-allowed"
              >
                Next
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function TxRow({ tx }: { tx: Transaction }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copyToClipboard(tx.tx_hash);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const typeColors: Record<string, string> = {
    deposit: "badge-success",
    transfer: "badge-info",
    shielded: "badge-purple",
    withdrawal: "badge-warning",
  };

  const statusColors: Record<string, string> = {
    pending: "badge-warning",
    included: "badge-info",
    executed: "badge-success",
    settled: "badge-success",
    failed: "badge-error",
  };

  return (
    <tr>
      <td className="font-mono text-sm">
        <span className="text-accent-cyan">{truncateHash(tx.tx_hash, 10)}</span>
      </td>
      <td>
        <span className={`badge ${typeColors[tx.tx_type] || "badge-info"}`}>
          {tx.tx_type.toUpperCase()}
        </span>
      </td>
      <td>
        <span className={`badge ${statusColors[tx.status] || "badge-info"}`}>
          {tx.status.toUpperCase()}
        </span>
      </td>
      <td>
        {tx.batch_id !== undefined ? (
          <span className="text-accent-purple">#{tx.batch_id}</span>
        ) : (
          <span className="text-text-muted">—</span>
        )}
      </td>
      <td className="text-right font-mono text-sm">
        {tx.amount !== undefined ? (
          formatBalance(tx.amount)
        ) : (
          <span className="text-text-muted">—</span>
        )}
      </td>
      <td className="text-text-secondary text-sm">{timeAgo(tx.received_at)}</td>
      <td>
        <button
          onClick={handleCopy}
          className="p-1.5 text-text-muted hover:text-text-primary transition-colors"
          title="Copy Hash"
        >
          {copied ? (
            <Check size={14} className="text-accent-green" />
          ) : (
            <Copy size={14} />
          )}
        </button>
      </td>
    </tr>
  );
}
