/**
 * WebSocket Provider
 *
 * Provides WebSocket connection and toast notifications to the entire app.
 */

import { createContext, useContext, ReactNode } from "react";
import { useWebSocket, type ToastEvent } from "./useWebSocket";
import { useToasts } from "./useToasts";
import type { Stats } from "./api";

interface WebSocketContextValue {
  connected: boolean;
  stats: Stats | null;
  lastUpdate: number | null;
}

const WebSocketContext = createContext<WebSocketContextValue>({
  connected: false,
  stats: null,
  lastUpdate: null,
});

export function useWebSocketContext() {
  return useContext(WebSocketContext);
}

interface WebSocketProviderProps {
  children: ReactNode;
}

export function WebSocketProvider({ children }: WebSocketProviderProps) {
  const { addToast, ToastRenderer } = useToasts();

  const handleToast = (event: ToastEvent) => {
    addToast(event);
  };

  const { connected, stats, lastUpdate } = useWebSocket({
    onToast: handleToast,
  });

  return (
    <WebSocketContext.Provider value={{ connected, stats, lastUpdate }}>
      {children}
      <ToastRenderer />
    </WebSocketContext.Provider>
  );
}
