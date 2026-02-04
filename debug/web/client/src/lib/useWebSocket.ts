/**
 * WebSocket Hook for Real-Time Updates
 *
 * Connects to the debug dashboard server's WebSocket endpoint
 * and provides real-time data updates to the UI.
 */

import { useEffect, useRef, useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { Stats, Batch, Transaction } from "./api";

// WebSocket message types
export interface WsStatsMessage {
  type: "stats";
  data: Stats;
  changes?: Partial<Stats>;
}

export interface WsBatchMessage {
  type: "batch";
  action: "created" | "updated" | "settled";
  data: Batch;
}

export interface WsTransactionMessage {
  type: "transaction";
  action: "created" | "updated" | "executed" | "settled" | "failed";
  data: Transaction;
}

export interface WsBatchesMessage {
  type: "batches";
  data: Batch[];
}

export interface WsTransactionsMessage {
  type: "transactions";
  data: Transaction[];
}

export interface WsPongMessage {
  type: "pong";
}

export type WsMessage =
  | WsStatsMessage
  | WsBatchMessage
  | WsTransactionMessage
  | WsBatchesMessage
  | WsTransactionsMessage
  | WsPongMessage;

// Event types for toast notifications
export interface ToastEvent {
  id: string;
  type: "success" | "info" | "warning" | "error";
  title: string;
  message: string;
  txHash?: string;
  l1TxSig?: string;
  timestamp: number;
}

interface UseWebSocketOptions {
  onToast?: (event: ToastEvent) => void;
}

interface UseWebSocketReturn {
  connected: boolean;
  stats: Stats | null;
  lastUpdate: number | null;
}

export function useWebSocket(options: UseWebSocketOptions = {}): UseWebSocketReturn {
  const { onToast } = options;
  const queryClient = useQueryClient();
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const pingIntervalRef = useRef<number | null>(null);

  const [connected, setConnected] = useState(false);
  const [stats, setStats] = useState<Stats | null>(null);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);

  // Track previous batch states for detecting transitions
  const prevBatchStatesRef = useRef<Map<number, string>>(new Map());
  const prevTxStatesRef = useRef<Map<string, string>>(new Map());

  const connect = useCallback(() => {
    // Determine WebSocket URL based on current location
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const host = window.location.host;
    const wsUrl = `${protocol}//${host}/ws`;

    console.log("[WS] Connecting to", wsUrl);

    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      console.log("[WS] Connected");
      setConnected(true);

      // Start ping interval
      pingIntervalRef.current = window.setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: "ping" }));
        }
      }, 30000);

      // Subscribe to all channels
      ws.send(JSON.stringify({ type: "subscribe", channel: "stats" }));
      ws.send(JSON.stringify({ type: "subscribe", channel: "batches" }));
      ws.send(JSON.stringify({ type: "subscribe", channel: "transactions" }));
    };

    ws.onmessage = (event) => {
      try {
        const message: WsMessage = JSON.parse(event.data);
        setLastUpdate(Date.now());

        switch (message.type) {
          case "stats":
            setStats(message.data);
            // Invalidate stats query to sync React Query cache
            queryClient.setQueryData(["stats"], message.data);
            break;

          case "batch": {
            const batch = message.data;
            const prevState = prevBatchStatesRef.current.get(batch.batch_id);

            // Check for state transitions and emit toasts
            if (message.action === "settled" || (prevState && prevState !== "settled" && batch.status === "settled")) {
              onToast?.({
                id: `batch-${batch.batch_id}-settled`,
                type: "success",
                title: "Batch Settled",
                message: `Batch #${batch.batch_id} settled on Solana`,
                l1TxSig: batch.l1_tx_sig,
                timestamp: Date.now(),
              });
            } else if (message.action === "created") {
              onToast?.({
                id: `batch-${batch.batch_id}-created`,
                type: "info",
                title: "New Batch",
                message: `Batch #${batch.batch_id} created with ${batch.tx_count} transactions`,
                timestamp: Date.now(),
              });
            }

            prevBatchStatesRef.current.set(batch.batch_id, batch.status);

            // Invalidate batches queries
            queryClient.invalidateQueries({ queryKey: ["batches"] });
            break;
          }

          case "transaction": {
            const tx = message.data;
            const prevStatus = prevTxStatesRef.current.get(tx.tx_hash);

            // Emit toasts for transaction status changes
            if (message.action === "created") {
              const typeLabel = tx.tx_type.charAt(0).toUpperCase() + tx.tx_type.slice(1);
              onToast?.({
                id: `tx-${tx.tx_hash}-created`,
                type: "info",
                title: `New ${typeLabel}`,
                message: `${typeLabel} transaction received`,
                txHash: tx.tx_hash,
                timestamp: Date.now(),
              });
            } else if (message.action === "executed" || (prevStatus && prevStatus !== "executed" && tx.status === "executed")) {
              const typeLabel = tx.tx_type.charAt(0).toUpperCase() + tx.tx_type.slice(1);
              onToast?.({
                id: `tx-${tx.tx_hash}-executed`,
                type: "success",
                title: `${typeLabel} Executed`,
                message: `Transaction included in batch`,
                txHash: tx.tx_hash,
                timestamp: Date.now(),
              });
            } else if (message.action === "settled" || (prevStatus && prevStatus !== "settled" && tx.status === "settled")) {
              const typeLabel = tx.tx_type.charAt(0).toUpperCase() + tx.tx_type.slice(1);
              onToast?.({
                id: `tx-${tx.tx_hash}-settled`,
                type: "success",
                title: `${typeLabel} Settled`,
                message: `Transaction settled on Solana`,
                txHash: tx.tx_hash,
                timestamp: Date.now(),
              });
            } else if (message.action === "failed" || tx.status === "failed") {
              onToast?.({
                id: `tx-${tx.tx_hash}-failed`,
                type: "error",
                title: "Transaction Failed",
                message: `Transaction execution failed`,
                txHash: tx.tx_hash,
                timestamp: Date.now(),
              });
            }

            prevTxStatesRef.current.set(tx.tx_hash, tx.status);

            // Invalidate transactions queries
            queryClient.invalidateQueries({ queryKey: ["transactions"] });
            break;
          }

          case "batches":
            // Bulk update - update cache
            message.data.forEach((batch) => {
              prevBatchStatesRef.current.set(batch.batch_id, batch.status);
            });
            queryClient.invalidateQueries({ queryKey: ["batches"] });
            break;

          case "transactions":
            // Bulk update - update cache
            message.data.forEach((tx) => {
              prevTxStatesRef.current.set(tx.tx_hash, tx.status);
            });
            queryClient.invalidateQueries({ queryKey: ["transactions"] });
            break;

          case "pong":
            // Heartbeat response, connection is alive
            break;
        }
      } catch (e) {
        console.error("[WS] Failed to parse message:", e);
      }
    };

    ws.onclose = () => {
      console.log("[WS] Disconnected");
      setConnected(false);

      // Clear ping interval
      if (pingIntervalRef.current) {
        clearInterval(pingIntervalRef.current);
        pingIntervalRef.current = null;
      }

      // Reconnect after delay
      reconnectTimeoutRef.current = window.setTimeout(() => {
        console.log("[WS] Reconnecting...");
        connect();
      }, 2000);
    };

    ws.onerror = (error) => {
      console.error("[WS] Error:", error);
    };
  }, [queryClient, onToast]);

  useEffect(() => {
    connect();

    return () => {
      if (wsRef.current) {
        wsRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
      if (pingIntervalRef.current) {
        clearInterval(pingIntervalRef.current);
      }
    };
  }, [connect]);

  return {
    connected,
    stats,
    lastUpdate,
  };
}
