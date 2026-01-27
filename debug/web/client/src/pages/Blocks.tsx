import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, Block } from "../lib/api";
import { truncateHash, formatTimestamp, copyToClipboard } from "../lib/formatters";
import { Copy, Check } from "lucide-react";

export default function Blocks() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["blocks", page, limit],
    queryFn: () => api.getBlocks(page * limit, limit),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Blocks</h1>
        <span className="text-text-muted text-sm">
          {data?.total ?? 0} total blocks
        </span>
      </div>

      <div className="card">
        <table className="data-table">
          <thead>
            <tr>
              <th>Batch ID</th>
              <th>Previous Root</th>
              <th>New Root</th>
              <th>Tx Count</th>
              <th>Open At</th>
              <th>Flags</th>
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
              data.items.map((block) => (
                <BlockRow key={block.batch_id} block={block} />
              ))
            ) : (
              <tr>
                <td colSpan={6} className="text-center py-8 text-text-muted">
                  No blocks found
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

function BlockRow({ block }: { block: Block }) {
  return (
    <tr>
      <td>
        <span className="text-accent-cyan font-medium">#{block.batch_id}</span>
      </td>
      <td className="font-mono text-xs">
        <HashCell hash={block.prev_root} />
      </td>
      <td className="font-mono text-xs">
        <HashCell hash={block.new_root} color="green" />
      </td>
      <td className="text-text-secondary">
        {block.tx_count}
      </td>
      <td className="text-text-secondary text-sm">
        {formatTimestamp(block.open_at)}
      </td>
      <td className="text-text-muted font-mono text-xs">
        0x{block.flags.toString(16).padStart(8, '0')}
      </td>
    </tr>
  );
}

function HashCell({
  hash,
  color = "default",
}: {
  hash: string;
  color?: "default" | "green" | "purple";
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copyToClipboard(hash);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const colorClass =
    color === "green"
      ? "text-accent-green"
      : color === "purple"
        ? "text-accent-purple"
        : "text-text-secondary";

  return (
    <div className="flex items-center gap-1">
      <span className={colorClass}>{truncateHash(hash, 10)}</span>
      <button
        onClick={handleCopy}
        className="p-1 text-text-muted hover:text-text-primary transition-colors"
        title="Copy"
      >
        {copied ? (
          <Check size={12} className="text-accent-green" />
        ) : (
          <Copy size={12} />
        )}
      </button>
    </div>
  );
}
