import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { formatBalance, truncateHash, copyToClipboard } from "../lib/formatters";
import { Copy, Check, Search } from "lucide-react";

export default function Accounts() {
  const [page, setPage] = useState(0);
  const [search, setSearch] = useState("");
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["accounts", page, limit],
    queryFn: () => api.getAccounts(page * limit, limit),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  const filteredItems = data?.items.filter(
    (account) =>
      search === "" ||
      account.id.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Accounts</h1>
        <div className="relative">
          <Search
            size={16}
            className="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted"
          />
          <input
            type="text"
            placeholder="Search by ID..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-9 pr-4 py-2 bg-bg-tertiary border border-border rounded text-sm focus:outline-none focus:border-accent-green w-64"
          />
        </div>
      </div>

      <div className="card">
        <table className="data-table">
          <thead>
            <tr>
              <th className="w-16">#</th>
              <th>Account ID</th>
              <th className="text-right">Balance</th>
              <th className="text-right">Nonce</th>
              <th className="w-16"></th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={5} className="text-center py-8 text-text-muted">
                  Loading...
                </td>
              </tr>
            ) : filteredItems && filteredItems.length > 0 ? (
              filteredItems.map((account, idx) => (
                <AccountRow
                  key={account.id}
                  account={account}
                  index={page * limit + idx + 1}
                />
              ))
            ) : (
              <tr>
                <td colSpan={5} className="text-center py-8 text-text-muted">
                  No accounts found
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

function AccountRow({
  account,
  index,
}: {
  account: { id: string; balance: number; nonce: number };
  index: number;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copyToClipboard(account.id);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <tr>
      <td className="text-text-muted">{index}</td>
      <td className="font-mono text-sm">
        <span className="text-accent-green">{account.id.slice(0, 8)}</span>
        <span className="text-text-muted">{account.id.slice(8, 56)}</span>
        <span className="text-accent-green">{account.id.slice(56)}</span>
      </td>
      <td className="text-right font-mono">
        {account.balance > 0 ? (
          <span className="text-accent-green">
            {formatBalance(account.balance)}
          </span>
        ) : (
          <span className="text-text-muted">0</span>
        )}
      </td>
      <td className="text-right text-text-secondary">{account.nonce}</td>
      <td>
        <button
          onClick={handleCopy}
          className="p-1.5 text-text-muted hover:text-text-primary transition-colors"
          title="Copy ID"
        >
          {copied ? <Check size={14} className="text-accent-green" /> : <Copy size={14} />}
        </button>
      </td>
    </tr>
  );
}
