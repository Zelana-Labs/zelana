/**
 * Utility functions for formatting data
 */

export function formatBalance(lamports: number): string {
  const sol = lamports / 1_000_000_000;
  if (sol >= 1) {
    return `${sol.toLocaleString(undefined, { maximumFractionDigits: 4 })} SOL`;
  }
  return `${lamports.toLocaleString()} lamports`;
}

export function formatNumber(n: number): string {
  return n.toLocaleString();
}

export function truncateHash(hash: string, chars = 8): string {
  if (hash.length <= chars * 2 + 2) return hash;
  return `${hash.slice(0, chars)}...${hash.slice(-chars)}`;
}

export function formatTimestamp(ts: number): string {
  const date = new Date(ts * 1000);
  return date.toLocaleString();
}

export function timeAgo(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts);

  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

export function copyToClipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

export function getTxTypeColor(
  type: string
): "green" | "blue" | "purple" | "yellow" {
  switch (type) {
    case "deposit":
      return "green";
    case "transfer":
      return "blue";
    case "shielded":
      return "purple";
    case "withdrawal":
      return "yellow";
    default:
      return "blue";
  }
}

export function getStatusColor(
  status: string
): "green" | "blue" | "yellow" | "red" {
  switch (status) {
    case "settled":
    case "executed":
      return "green";
    case "included":
    case "proving":
      return "blue";
    case "pending":
    case "building":
    case "pending_settlement":
      return "yellow";
    case "failed":
      return "red";
    default:
      return "blue";
  }
}
