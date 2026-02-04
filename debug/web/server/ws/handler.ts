/**
 * WebSocket Handler
 *
 * Provides real-time updates to connected clients by polling the DB
 * and pushing changes for stats, batches, and transactions.
 */

import type { DbReaderClient } from "../db/client";

interface WebSocketData {
  subscriptions: Set<string>;
}

// Store connected clients
const clients = new Set<any>();

// Previous data for diff detection
let previousStats: Record<string, unknown> | null = null;
let previousBatches: Map<number, BatchData> = new Map();
let previousTransactions: Map<string, TransactionData> = new Map();

interface BatchData {
  batch_id: number;
  tx_count: number;
  state_root: string;
  shielded_root: string;
  l1_tx_sig?: string;
  status: string;
  created_at: number;
  settled_at?: number;
}

interface TransactionData {
  tx_hash: string;
  tx_type: string;
  batch_id?: number;
  status: string;
  received_at: number;
  executed_at?: number;
  amount?: number;
  from?: string;
  to?: string;
}

export function createWebSocketHandler(dbClient: DbReaderClient) {
  // Start polling for stats updates (every 500ms)
  setInterval(async () => {
    if (clients.size === 0) return;
    if (!dbClient.isConnected()) return;

    try {
      const stats = (await dbClient.request({ cmd: "stats" })) as Record<
        string,
        unknown
      >;

      // Check for changes
      if (previousStats) {
        const changes: Record<string, unknown> = {};
        let hasChanges = false;

        for (const key of Object.keys(stats)) {
          if (stats[key] !== previousStats[key]) {
            changes[key] = stats[key];
            hasChanges = true;
          }
        }

        if (hasChanges) {
          broadcast({ type: "stats", data: stats, changes });
        }
      } else {
        // First time, send initial stats
        broadcast({ type: "stats", data: stats });
      }

      previousStats = stats;
    } catch (e) {
      // Ignore errors during polling
    }
  }, 500);

  // Start polling for batch updates (every 500ms)
  setInterval(async () => {
    if (clients.size === 0) return;
    if (!dbClient.isConnected()) return;

    try {
      const result = (await dbClient.request({
        cmd: "batches",
        offset: 0,
        limit: 50,
      })) as { items: BatchData[] };

      if (!result.items) return;

      for (const batch of result.items) {
        const prev = previousBatches.get(batch.batch_id);

        if (!prev) {
          // New batch created
          broadcast({
            type: "batch",
            action: "created",
            data: batch,
          });
        } else if (prev.status !== batch.status) {
          // Batch status changed
          const action =
            batch.status === "settled"
              ? "settled"
              : batch.status === "failed"
              ? "updated"
              : "updated";
          broadcast({
            type: "batch",
            action,
            data: batch,
          });
        } else if (prev.l1_tx_sig !== batch.l1_tx_sig && batch.l1_tx_sig) {
          // L1 tx sig added (settlement)
          broadcast({
            type: "batch",
            action: "settled",
            data: batch,
          });
        }

        previousBatches.set(batch.batch_id, batch);
      }
    } catch (e) {
      // Ignore errors during polling
    }
  }, 500);

  // Start polling for transaction updates (every 500ms)
  setInterval(async () => {
    if (clients.size === 0) return;
    if (!dbClient.isConnected()) return;

    try {
      const result = (await dbClient.request({
        cmd: "transactions",
        offset: 0,
        limit: 100,
      })) as { items: TransactionData[] };

      if (!result.items) return;

      for (const tx of result.items) {
        const prev = previousTransactions.get(tx.tx_hash);

        if (!prev) {
          // New transaction received
          broadcast({
            type: "transaction",
            action: "created",
            data: tx,
          });
        } else if (prev.status !== tx.status) {
          // Transaction status changed
          let action: string = "updated";
          if (tx.status === "executed") {
            action = "executed";
          } else if (tx.status === "settled") {
            action = "settled";
          } else if (tx.status === "failed") {
            action = "failed";
          }
          broadcast({
            type: "transaction",
            action,
            data: tx,
          });
        }

        previousTransactions.set(tx.tx_hash, tx);
      }
    } catch (e) {
      // Ignore errors during polling
    }
  }, 500);

  return {
    open(ws: any) {
      clients.add(ws);
      console.log(`WebSocket client connected (${clients.size} total)`);

      // Send initial data
      if (dbClient.isConnected()) {
        // Send stats
        dbClient
          .request({ cmd: "stats" })
          .then((stats) => {
            ws.send(JSON.stringify({ type: "stats", data: stats }));
          })
          .catch(() => {});

        // Send recent batches
        dbClient
          .request({ cmd: "batches", offset: 0, limit: 20 })
          .then((result: any) => {
            if (result.items) {
              ws.send(JSON.stringify({ type: "batches", data: result.items }));
              // Update cache
              for (const batch of result.items) {
                previousBatches.set(batch.batch_id, batch);
              }
            }
          })
          .catch(() => {});

        // Send recent transactions
        dbClient
          .request({ cmd: "transactions", offset: 0, limit: 50 })
          .then((result: any) => {
            if (result.items) {
              ws.send(
                JSON.stringify({ type: "transactions", data: result.items })
              );
              // Update cache
              for (const tx of result.items) {
                previousTransactions.set(tx.tx_hash, tx);
              }
            }
          })
          .catch(() => {});
      }
    },

    message(ws: any, message: string | Buffer) {
      try {
        const data = JSON.parse(message.toString());

        if (data.type === "subscribe") {
          // Handle subscription to specific events
          const wsData = ws.data as WebSocketData;
          wsData.subscriptions = wsData.subscriptions || new Set();
          wsData.subscriptions.add(data.channel);
        }

        if (data.type === "ping") {
          ws.send(JSON.stringify({ type: "pong" }));
        }
      } catch (e) {
        // Ignore invalid messages
      }
    },

    close(ws: any) {
      clients.delete(ws);
      console.log(`WebSocket client disconnected (${clients.size} total)`);
    },
  };
}

function broadcast(message: unknown) {
  const json = JSON.stringify(message);
  for (const client of clients) {
    try {
      client.send(json);
    } catch (e) {
      clients.delete(client);
    }
  }
}
