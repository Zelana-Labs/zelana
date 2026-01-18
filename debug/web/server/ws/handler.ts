/**
 * WebSocket Handler
 *
 * Provides real-time updates to connected clients by polling the DB
 * and pushing changes.
 */

import type { DbReaderClient } from "../db/client";

interface WebSocketData {
  subscriptions: Set<string>;
}

// Store connected clients
const clients = new Set<any>();

// Previous stats for diff detection
let previousStats: Record<string, unknown> | null = null;

export function createWebSocketHandler(dbClient: DbReaderClient) {
  // Start polling for updates
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
      }

      previousStats = stats;
    } catch (e) {
      // Ignore errors during polling
    }
  }, 500);

  return {
    open(ws: any) {
      clients.add(ws);
      console.log(`WebSocket client connected (${clients.size} total)`);

      // Send initial stats
      if (dbClient.isConnected()) {
        dbClient
          .request({ cmd: "stats" })
          .then((stats) => {
            ws.send(JSON.stringify({ type: "stats", data: stats }));
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
