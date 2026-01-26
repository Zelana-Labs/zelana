import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { truncateHash, copyToClipboard, formatNumber } from "../lib/formatters";
import { Copy, Check, Hash, Shield, FileText } from "lucide-react";

type Tab = "nullifiers" | "commitments" | "notes" | "tree";

export default function Shielded() {
  const [activeTab, setActiveTab] = useState<Tab>("nullifiers");

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Shielded State</h1>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        <TabButton
          active={activeTab === "nullifiers"}
          onClick={() => setActiveTab("nullifiers")}
          icon={Hash}
          label="Nullifiers"
        />
        <TabButton
          active={activeTab === "commitments"}
          onClick={() => setActiveTab("commitments")}
          icon={Shield}
          label="Commitments"
        />
        <TabButton
          active={activeTab === "notes"}
          onClick={() => setActiveTab("notes")}
          icon={FileText}
          label="Encrypted Notes"
        />
        <TabButton
          active={activeTab === "tree"}
          onClick={() => setActiveTab("tree")}
          icon={Shield}
          label="Tree Metadata"
        />
      </div>

      {activeTab === "nullifiers" && <NullifiersTab />}
      {activeTab === "commitments" && <CommitmentsTab />}
      {activeTab === "notes" && <EncryptedNotesTab />}
      {activeTab === "tree" && <TreeMetaTab />}
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
      className={`flex items-center gap-2 px-4 py-2.5 text-sm border-b-2 transition-colors ${active
          ? "border-accent-purple text-accent-purple"
          : "border-transparent text-text-secondary hover:text-text-primary"
        }`}
    >
      <Icon size={16} />
      {label}
    </button>
  );
}

function NullifiersTab() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["nullifiers", page, limit],
    queryFn: () => api.getNullifiers(page * limit, limit),

    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="card">
      <table className="data-table">
        <thead>
          <tr>
            <th className="w-16">#</th>
            <th>Nullifier Hash</th>
            <th className="w-16"></th>
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
            data.items.map((nf, idx) => (
              <HashRow
                key={nf.nullifier}
                index={page * limit + idx + 1}
                hash={nf.nullifier}
              />
            ))
          ) : (
            <tr>
              <td colSpan={3} className="text-center py-8 text-text-muted">
                No nullifiers found
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Pagination data={data} page={page} limit={limit} setPage={setPage} />
    </div>
  );
}

function CommitmentsTab() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["commitments", page, limit],
    queryFn: () => api.getCommitments(page * limit, limit),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="card">
      <table className="data-table">
        <thead>
          <tr>
            <th className="w-24">Position</th>
            <th>Commitment Hash</th>
            <th className="w-16"></th>
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
            data.items.map((cm) => (
              <tr key={cm.position}>
                <td className="text-accent-green font-mono">{cm.position}</td>
                <td className="font-mono text-sm">
                  <span className="text-accent-purple">
                    {cm.commitment.slice(0, 8)}
                  </span>
                  <span className="text-text-muted">
                    {cm.commitment.slice(8, 56)}
                  </span>
                  <span className="text-accent-purple">
                    {cm.commitment.slice(56)}
                  </span>
                </td>
                <td>
                  <CopyButton text={cm.commitment} />
                </td>
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={3} className="text-center py-8 text-text-muted">
                No commitments found
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Pagination data={data} page={page} limit={limit} setPage={setPage} />
    </div>
  );
}

function EncryptedNotesTab() {
  const [page, setPage] = useState(0);
  const limit = 25;

  const { data, isLoading } = useQuery({
    queryKey: ["encrypted_notes", page, limit],
    queryFn: () => api.getEncryptedNotes(page * limit, limit),
    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  return (
    <div className="card">
      <table className="data-table">
        <thead>
          <tr>
            <th>Commitment</th>
            <th>Ephemeral PK</th>
            <th className="text-right">Ciphertext Size</th>
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
            data.items.map((note) => (
              <tr key={note.commitment}>
                <td className="font-mono text-sm text-accent-purple">
                  {truncateHash(note.commitment, 12)}
                </td>
                <td className="font-mono text-sm text-text-secondary">
                  {truncateHash(note.ephemeral_pk, 8)}
                </td>
                <td className="text-right text-text-secondary">
                  {note.ciphertext_len} bytes
                </td>
                <td>
                  <CopyButton text={note.commitment} />
                </td>
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={4} className="text-center py-8 text-text-muted">
                No encrypted notes found
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Pagination data={data} page={page} limit={limit} setPage={setPage} />
    </div>
  );
}

function TreeMetaTab() {
  const { data, isLoading } = useQuery({
    queryKey: ["tree_meta"],
    queryFn: api.getTreeMeta,

    refetchInterval: 1000,          // 🔥 live updates
    refetchOnWindowFocus: true,     // 🔥 refetch when tab refocuses
    staleTime: 0,
  });

  if (isLoading) {
    return (
      <div className="card p-8 text-center text-text-muted">Loading...</div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="card p-4">
        <div className="text-text-muted text-sm mb-2">Next Position</div>
        <div className="text-2xl font-semibold text-accent-green">
          {formatNumber(data?.next_position ?? 0)}
        </div>
      </div>

      <div className="card">
        <div className="card-header">
          <h2 className="font-medium">Frontier Nodes</h2>
          <span className="text-text-muted text-sm">
            {data?.frontier?.length ?? 0} levels
          </span>
        </div>
        <table className="data-table">
          <thead>
            <tr>
              <th className="w-24">Level</th>
              <th>Hash</th>
              <th className="w-16"></th>
            </tr>
          </thead>
          <tbody>
            {data?.frontier && data.frontier.length > 0 ? (
              data.frontier.map((node) => (
                <tr key={node.level}>
                  <td className="text-accent-cyan font-mono">{node.level}</td>
                  <td className="font-mono text-sm text-text-secondary">
                    {truncateHash(node.hash, 16)}
                  </td>
                  <td>
                    <CopyButton text={node.hash} />
                  </td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={3} className="text-center py-8 text-text-muted">
                  No frontier nodes
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function HashRow({ index, hash }: { index: number; hash: string }) {
  return (
    <tr>
      <td className="text-text-muted">{index}</td>
      <td className="font-mono text-sm">
        <span className="text-accent-yellow">{hash.slice(0, 8)}</span>
        <span className="text-text-muted">{hash.slice(8, 56)}</span>
        <span className="text-accent-yellow">{hash.slice(56)}</span>
      </td>
      <td>
        <CopyButton text={hash} />
      </td>
    </tr>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await copyToClipboard(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button
      onClick={handleCopy}
      className="p-1.5 text-text-muted hover:text-text-primary transition-colors"
      title="Copy"
    >
      {copied ? (
        <Check size={14} className="text-accent-green" />
      ) : (
        <Copy size={14} />
      )}
    </button>
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
