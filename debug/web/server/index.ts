/**
 * Zelana Debug Dashboard - Backend Server
 *
 * A Bun.js server using Hono that:
 * 1. Connects to the Rust db-reader via TCP
 * 2. Exposes REST API endpoints for the frontend
 * 3. Provides WebSocket for real-time updates
 * 4. Checks sequencer and Solana RPC connectivity
 */

import { Hono } from "hono";
import { cors } from "hono/cors";
import { serveStatic } from "hono/bun";
import { DbReaderClient } from "./db/client";
import { createWebSocketHandler } from "./ws/handler";

const app = new Hono();

// Configuration
const PORT = parseInt(process.env.PORT || "3456");
const DB_READER_PORT = parseInt(process.env.DB_READER_PORT || "3457");
const SEQUENCER_URL = process.env.SEQUENCER_URL || "http://127.0.0.1:8080";
const SOLANA_RPC_URL = process.env.SOLANA_RPC_URL || "http://127.0.0.1:8899";

// Initialize DB reader client
const dbClient = new DbReaderClient("127.0.0.1", DB_READER_PORT);

// CORS for development
app.use(
  "/api/*",
  cors({
    origin: ["http://localhost:5173", "http://127.0.0.1:5173"],
  })
);

// Health check
app.get("/api/health", async (c) => {
  const dbConnected = dbClient.isConnected();

  // Check sequencer
  let sequencerConnected = false;
  try {
    const res = await fetch(`${SEQUENCER_URL}/health`, {
      signal: AbortSignal.timeout(2000),
    });
    sequencerConnected = res.ok;
  } catch {}

  // Check Solana RPC
  let solanaConnected = false;
  try {
    const res = await fetch(SOLANA_RPC_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "getHealth",
      }),
      signal: AbortSignal.timeout(2000),
    });
    const data = await res.json();
    solanaConnected = data.result === "ok";
  } catch {}

  return c.json({
    dbReader: dbConnected,
    sequencer: sequencerConnected,
    solanaRpc: solanaConnected,
    sequencerUrl: SEQUENCER_URL,
    solanaRpcUrl: SOLANA_RPC_URL,
  });
});

// Stats endpoint
app.get("/api/stats", async (c) => {
  try {
    const result = await dbClient.request({ cmd: "stats" });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Accounts
app.get("/api/accounts", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "accounts",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/accounts/:id", async (c) => {
  const id = c.req.param("id");

  try {
    const result = await dbClient.request({ cmd: "account", id });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Transactions
app.get("/api/transactions", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");
  const batch_id = c.req.query("batch_id")
    ? parseInt(c.req.query("batch_id")!)
    : undefined;
  const tx_type = c.req.query("tx_type") || undefined;
  const status = c.req.query("status") || undefined;

  try {
    const result = await dbClient.request({
      cmd: "transactions",
      offset,
      limit,
      batch_id,
      tx_type,
      status,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/transactions/:hash", async (c) => {
  const hash = c.req.param("hash");

  try {
    const result = await dbClient.request({ cmd: "transaction", hash });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Batches
app.get("/api/batches", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "batches",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/batches/:id", async (c) => {
  const id = parseInt(c.req.param("id"));

  try {
    const result = await dbClient.request({ cmd: "batch", id });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Blocks
app.get("/api/blocks", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "blocks",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Shielded state
app.get("/api/shielded/nullifiers", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "nullifiers",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/shielded/commitments", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "commitments",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/shielded/notes", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "encrypted_notes",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/shielded/tree", async (c) => {
  try {
    const result = await dbClient.request({ cmd: "tree_meta" });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Bridge
app.get("/api/bridge/deposits", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "deposits",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

app.get("/api/bridge/withdrawals", async (c) => {
  const offset = parseInt(c.req.query("offset") || "0");
  const limit = parseInt(c.req.query("limit") || "50");

  try {
    const result = await dbClient.request({
      cmd: "withdrawals",
      offset,
      limit,
    });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Indexer
app.get("/api/indexer", async (c) => {
  try {
    const result = await dbClient.request({ cmd: "indexer_meta" });
    return c.json(result);
  } catch (e) {
    return c.json({ error: String(e) }, 500);
  }
});

// Serve static files in production
app.use("/*", serveStatic({ root: "./client/dist" }));
app.get("/*", serveStatic({ path: "./client/dist/index.html" }));

// Start server
console.log(`
╔═══════════════════════════════════════════════════════════╗
║           Zelana Debug Dashboard Server                   ║
╠═══════════════════════════════════════════════════════════╣
║  API Server:     http://localhost:${PORT}                    ║
║  DB Reader:      127.0.0.1:${DB_READER_PORT}                        ║
║  Sequencer:      ${SEQUENCER_URL.padEnd(35)}  ║
║  Solana RPC:     ${SOLANA_RPC_URL.padEnd(35)}  ║
╚═══════════════════════════════════════════════════════════╝
`);

// Connect to DB reader
dbClient.connect().catch((e) => {
  console.error("Failed to connect to DB reader:", e);
  console.log("Make sure to start the db-reader first:");
  console.log("  cargo run -p db-reader --release");
});

// Create WebSocket upgrade handler
const wsHandler = createWebSocketHandler(dbClient);

// Export for Bun
export default {
  port: PORT,
  fetch: app.fetch,
  websocket: wsHandler,
};
