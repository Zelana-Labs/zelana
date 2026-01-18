/**
 * Zelana React Context
 * 
 * Provides the ZelanaClient to all child components.
 */

import React, { createContext, useContext, useMemo, useState, useCallback, useEffect } from 'react';
import { 
  ZelanaClient, 
  Keypair, 
  type ZelanaClientConfig,
  type AccountState,
} from '@zelana/sdk';

// ============================================================================
// Types
// ============================================================================

export interface ZelanaContextValue {
  /** The Zelana client instance */
  client: ZelanaClient | null;
  /** Current keypair (if connected) */
  keypair: Keypair | null;
  /** Whether the client is connected */
  isConnected: boolean;
  /** Whether currently loading */
  isLoading: boolean;
  /** Last error */
  error: Error | null;
  /** Connect with a keypair */
  connect: (keypair: Keypair) => void;
  /** Disconnect the current keypair */
  disconnect: () => void;
  /** Clear error */
  clearError: () => void;
}

export interface ZelanaProviderProps {
  /** Base URL of the sequencer */
  baseUrl: string;
  /** Optional chain ID (default: 1) */
  chainId?: bigint;
  /** Auto-reconnect on mount if keypair in storage */
  autoReconnect?: boolean;
  /** Storage key for persisting keypair (default: 'zelana-keypair') */
  storageKey?: string;
  /** Children */
  children: React.ReactNode;
}

// ============================================================================
// Context
// ============================================================================

const ZelanaContext = createContext<ZelanaContextValue | null>(null);

/**
 * Hook to access the Zelana context
 */
export function useZelanaContext(): ZelanaContextValue {
  const context = useContext(ZelanaContext);
  if (!context) {
    throw new Error('useZelanaContext must be used within a ZelanaProvider');
  }
  return context;
}

// ============================================================================
// Provider
// ============================================================================

/**
 * Zelana Provider Component
 * 
 * Wrap your app with this provider to enable Zelana hooks.
 * 
 * @example
 * ```tsx
 * import { ZelanaProvider } from '@zelana/react';
 * 
 * function App() {
 *   return (
 *     <ZelanaProvider baseUrl="http://localhost:3000">
 *       <MyApp />
 *     </ZelanaProvider>
 *   );
 * }
 * ```
 */
export function ZelanaProvider({
  baseUrl,
  chainId = 1n,
  autoReconnect = false,
  storageKey = 'zelana-keypair',
  children,
}: ZelanaProviderProps) {
  const [keypair, setKeypair] = useState<Keypair | null>(null);
  const [isLoading, setIsLoading] = useState(autoReconnect);
  const [error, setError] = useState<Error | null>(null);

  // Create client when keypair changes
  const client = useMemo(() => {
    if (!keypair) return null;
    return new ZelanaClient({
      baseUrl,
      chainId,
      keypair,
    });
  }, [baseUrl, chainId, keypair]);

  // Connect with a keypair
  const connect = useCallback((kp: Keypair) => {
    setKeypair(kp);
    setError(null);
    
    // Persist to storage
    if (typeof localStorage !== 'undefined') {
      try {
        localStorage.setItem(storageKey, kp.secretKeyHex);
      } catch {
        // Storage might be unavailable
      }
    }
  }, [storageKey]);

  // Disconnect
  const disconnect = useCallback(() => {
    setKeypair(null);
    setError(null);
    
    // Remove from storage
    if (typeof localStorage !== 'undefined') {
      try {
        localStorage.removeItem(storageKey);
      } catch {
        // Storage might be unavailable
      }
    }
  }, [storageKey]);

  // Clear error
  const clearError = useCallback(() => {
    setError(null);
  }, []);

  // Auto-reconnect on mount
  useEffect(() => {
    if (!autoReconnect) return;
    
    const stored = typeof localStorage !== 'undefined' 
      ? localStorage.getItem(storageKey) 
      : null;
    
    if (stored) {
      try {
        const kp = Keypair.fromHex(stored);
        setKeypair(kp);
      } catch (e) {
        setError(e instanceof Error ? e : new Error('Failed to restore keypair'));
      }
    }
    
    setIsLoading(false);
  }, [autoReconnect, storageKey]);

  const value: ZelanaContextValue = {
    client,
    keypair,
    isConnected: !!keypair,
    isLoading,
    error,
    connect,
    disconnect,
    clearError,
  };

  return (
    <ZelanaContext.Provider value={value}>
      {children}
    </ZelanaContext.Provider>
  );
}
