import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { truncateHash, copyToClipboard, formatNumber } from "../lib/formatters";
import { Copy, Check, Download, Upload, Database } from "lucide-react";

type Tab = "deposits" | "withdrawals" | "indexer";

export default function Bridge() {
  const [activeTab, setActiveTab] = useState<Tab>("deposits");

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Bridge</h1>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        <TabButton
          active={activeTab === "deposits"}
          onClick={() => setActiveTab("deposits")}
          icon={Download}
          label="Processed Deposits"
        />
        <TabButton
          active={activeTab === "withdrawals"}
          onClick={() => setActiveTab("withdrawals")}
          icon={Upload}
          label="Pending Withdrawals"
        />
        <TabButton
          active={activeTab === "indexer"}
          onClick={() => setActiveTab("indexer")}
          icon={Database}
          label="Indexer Status"
        />
      </div>

      {activeTab === "deposits" && <DepositsTab />}
      {activeTab === "withdrawals" && <WithdrawalsTab />}
      {activeTab === "indexer" && <IndexerTab />}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon: Icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ElementType;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-2 px-4 py-2.5 text-sm border-b-2 transition-colors ${
        active
          ? "border-accent-green text-accent-green"
          : "border-transparent text-text-secondary hover:text-text-primary"
      }`}
    >
      <Icon size={16} />
      {label}
    </button>
  );
}

function DepositsTab() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["deposits", page, limit],
    queryFn: () => api.getDeposits(page * limit, limit),
      refetchInterval: 1000,          // 🔥 live updates
  refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
  staleTime: 0,   
  });

  return (
    <div className="card">
      <table className="data-table">
        <thead>
          <tr>
            <th>L1 Sequence</th>
            <th>Processed at Slot</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {isLoading ? (
            <tr>
              <td colSpan={3} className="text-center py-8 text-text-muted">
                Loading...
              </td>
            </tr>
          ) : data && data.items.length > 0 ? (
            data.items.map((deposit) => (
              <tr key={deposit.l1_seq}>
                <td className="font-mono text-accent-green">
                  #{formatNumber(deposit.l1_seq)}
                </td>
                <td className="font-mono text-text-secondary">
                  {formatNumber(deposit.slot)}
                </td>
                <td>
                  <span className="badge badge-success">PROCESSED</span>
                </td>
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={3} className="text-center py-8 text-text-muted">
                No processed deposits found
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Pagination data={data} page={page} limit={limit} setPage={setPage} />
    </div>
  );
}

function WithdrawalsTab() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["withdrawals", page, limit],
    queryFn: () => api.getWithdrawals(page * limit, limit),
  });

  return (
    <div className="card">
      <table className="data-table">
        <thead>
          <tr>
            <th>Transaction Hash</th>
            <th className="text-right">Data Size</th>
            <th>Status</th>
            <th className="w-16"></th>
          </tr>
        </thead>
        <tbody>
          {isLoading ? (
            <tr>
              <td colSpan={4} className="text-center py-8 text-text-muted">
                Loading...
              </td>
            </tr>
          ) : data && data.items.length > 0 ? (
            data.items.map((withdrawal) => (
              <WithdrawalRow key={withdrawal.tx_hash} withdrawal={withdrawal} />
            ))
          ) : (
            <tr>
              <td colSpan={4} className="text-center py-8 text-text-muted">
                No pending withdrawals
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Pagination data={data} page={page} limit={limit} setPage={setPage} />
    </div>
  );
}

function WithdrawalRow({
  withdrawal,
}: {
  withdrawal: { tx_hash: string; data_len: number };
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copyToClipboard(withdrawal.tx_hash);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <tr>
      <td className="font-mono text-sm">
        <span className="text-accent-yellow">
          {truncateHash(withdrawal.tx_hash, 12)}
        </span>
      </td>
      <td className="text-right text-text-secondary">
        {withdrawal.data_len} bytes
      </td>
      <td>
        <span className="badge badge-warning">PENDING</span>
      </td>
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

function IndexerTab() {
  const { data, isLoading } = useQuery({
    queryKey: ["indexer_meta"],
    queryFn: api.getIndexerMeta,
  });

  if (isLoading) {
    return (
      <div className="card p-8 text-center text-text-muted">Loading...</div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-4">
      <div className="card p-6">
        <div className="text-text-muted text-sm mb-2">Last Processed Slot</div>
        <div className="text-3xl font-semibold text-accent-cyan">
          {data?.last_processed_slot !== undefined
            ? formatNumber(data.last_processed_slot)
            : "—"}
        </div>
        <div className="text-text-muted text-xs mt-2">
          The last L1 slot that was processed for deposit events
        </div>
      </div>

      <div className="card p-6">
        <div className="text-text-muted text-sm mb-2">Indexer Status</div>
        <div className="flex items-center gap-2">
          <div
            className={`w-3 h-3 rounded-full ${
              data?.last_processed_slot !== undefined
                ? "bg-accent-green animate-pulse"
                : "bg-accent-yellow"
            }`}
          />
          <span className="text-lg font-medium">
            {data?.last_processed_slot !== undefined ? "Active" : "Inactive"}
          </span>
        </div>
        <div className="text-text-muted text-xs mt-2">
          Watches L1 for deposit events and credits L2 accounts
        </div>
      </div>
    </div>
  );
}

function Pagination({
  data,
  page,
  limit,
  setPage,
}: {
  data: { total: number } | undefined;
  page: number;
  limit: number;
  setPage: (fn: (p: number) => number) => void;
}) {
  if (!data || data.total <= limit) return null;

  return (
    <div className="px-4 py-3 border-t border-border flex items-center justify-between">
      <div className="text-sm text-text-muted">
        Showing {page * limit + 1} - {Math.min((page + 1) * limit, data.total)}{" "}
        of {data.total}
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
  );
}
