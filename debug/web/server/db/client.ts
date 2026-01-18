/**
 * DB Reader Client
 *
 * TCP client that communicates with the Rust db-reader server.
 * Sends JSON commands over a persistent connection.
 */

import { Socket } from "net";

interface DbRequest {
  cmd: string;
  [key: string]: unknown;
}

interface DbResponse {
  success: boolean;
  data?: unknown;
  error?: string;
}

export class DbReaderClient {
  private host: string;
  private port: number;
  private socket: Socket | null = null;
  private connected = false;
  private pendingRequests: Map<
    number,
    {
      resolve: (value: unknown) => void;
      reject: (reason: unknown) => void;
    }
  > = new Map();
  private requestId = 0;
  private buffer = "";
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(host: string, port: number) {
    this.host = host;
    this.port = port;
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.socket = new Socket();

      this.socket.on("connect", () => {
        console.log(`Connected to DB reader at ${this.host}:${this.port}`);
        this.connected = true;
        resolve();
      });

      this.socket.on("data", (data) => {
        this.buffer += data.toString();
        this.processBuffer();
      });

      this.socket.on("error", (err) => {
        console.error("DB reader socket error:", err.message);
        this.connected = false;
        if (!this.socket) {
          reject(err);
        }
        this.scheduleReconnect();
      });

      this.socket.on("close", () => {
        console.log("DB reader connection closed");
        this.connected = false;
        this.scheduleReconnect();
      });

      this.socket.connect(this.port, this.host);
    });
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      console.log("Attempting to reconnect to DB reader...");
      this.connect().catch(() => {});
    }, 3000);
  }

  private processBuffer() {
    const lines = this.buffer.split("\n");
    this.buffer = lines.pop() || "";

    for (const line of lines) {
      if (!line.trim()) continue;

      try {
        const response: DbResponse = JSON.parse(line);
        // For now, we use a simple request-response pattern
        // The first pending request gets the response
        const [id, handler] = this.pendingRequests.entries().next().value || [];
        if (handler) {
          this.pendingRequests.delete(id);
          if (response.success) {
            handler.resolve(response.data);
          } else {
            handler.reject(new Error(response.error || "Unknown error"));
          }
        }
      } catch (e) {
        console.error("Failed to parse DB response:", e);
      }
    }
  }

  isConnected(): boolean {
    return this.connected;
  }

  async request(req: DbRequest): Promise<unknown> {
    if (!this.connected || !this.socket) {
      throw new Error("Not connected to DB reader");
    }

    const id = ++this.requestId;

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { resolve, reject });

      const json = JSON.stringify(req) + "\n";
      this.socket!.write(json, (err) => {
        if (err) {
          this.pendingRequests.delete(id);
          reject(err);
        }
      });

      // Timeout after 10 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error("Request timeout"));
        }
      }, 10000);
    });
  }

  close() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }
    this.connected = false;
  }
}
