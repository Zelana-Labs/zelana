import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, Batch } from "../lib/api";
import { truncateHash, timeAgo, formatTimestamp, copyToClipboard } from "../lib/formatters";
import { Copy, Check, ExternalLink } from "lucide-react";

export default function Batches() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["batches", page, limit],
    queryFn: () => api.getBatches(page * limit, limit),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Batches</h1>
        <span className="text-text-muted text-sm">
          {data?.total ?? 0} total batches
        </span>
      </div>

      <div className="card">
        <table className="data-table">
          <thead>
            <tr>
              <th>Batch ID</th>
              <th>Status</th>
              <th className="text-right">Tx Count</th>
              <th>State Root</th>
              <th>L1 Signature</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={6} className="text-center py-8 text-text-muted">
                  Loading...
                </td>
              </tr>
            ) : data && data.items.length > 0 ? (
              data.items.map((batch) => (
                <BatchRow key={batch.batch_id} batch={batch} />
              ))
            ) : (
              <tr>
                <td colSpan={6} className="text-center py-8 text-text-muted">
                  No batches found
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

function BatchRow({ batch }: { batch: Batch }) {
  const [copied, setCopied] = useState(false);

  const statusColors: Record<string, string> = {
    building: "badge-warning",
    proving: "badge-info",
    pending_settlement: "badge-warning",
    settled: "badge-success",
    failed: "badge-error",
  };

  const handleCopy = async () => {
    await copyToClipboard(batch.state_root);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <tr>
      <td>
        <span className="text-accent-purple font-medium">#{batch.batch_id}</span>
      </td>
      <td>
        <span className={`badge ${statusColors[batch.status] || "badge-info"}`}>
          {batch.status.replace("_", " ").toUpperCase()}
        </span>
      </td>
      <td className="text-right">{batch.tx_count}</td>
      <td className="font-mono text-sm">
        <div className="flex items-center gap-2">
          <span className="text-text-secondary">
            {truncateHash(batch.state_root, 12)}
          </span>
          <button
            onClick={handleCopy}
            className="p-1 text-text-muted hover:text-text-primary transition-colors"
            title="Copy State Root"
          >
            {copied ? (
              <Check size={12} className="text-accent-green" />
            ) : (
              <Copy size={12} />
            )}
          </button>
        </div>
      </td>
      <td className="font-mono text-sm">
        {batch.l1_tx_sig ? (
          <a
            href={`https://explorer.solana.com/tx/${batch.l1_tx_sig}?cluster=devnet`}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-accent-cyan hover:underline"
          >
            {truncateHash(batch.l1_tx_sig, 8)}
            <ExternalLink size={12} />
          </a>
        ) : (
          <span className="text-text-muted">—</span>
        )}
      </td>
      <td className="text-text-secondary text-sm">
        {timeAgo(batch.created_at)}
      </td>
    </tr>
  );
}
