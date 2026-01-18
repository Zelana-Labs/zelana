/**
 * Zelana React Hooks
 * 
 * React hooks for interacting with Zelana L2.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { useZelanaContext } from './context';
import type {
  AccountState,
  TransferResponse,
  WithdrawResponse,
  TxSummary,
  BatchSummary,
  HealthInfo,
  StateRoots,
  GlobalStats,
  BatchStatusInfo,
} from '@zelana/sdk';

// ============================================================================
// Types
// ============================================================================

export interface UseQueryResult<T> {
  data: T | null;
  isLoading: boolean;
  error: Error | null;
  refetch: () => Promise<void>;
}

export interface UseMutationResult<TData, TVariables> {
  data: TData | null;
  isLoading: boolean;
  error: Error | null;
  mutate: (variables: TVariables) => Promise<TData>;
  reset: () => void;
}

// ============================================================================
// Core Hooks
// ============================================================================

/**
 * Hook to access the Zelana client
 */
export function useZelana() {
  return useZelanaContext();
}

/**
 * Hook to check if sequencer is healthy
 */
export function useHealth(): UseQueryResult<HealthInfo> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<HealthInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const health = await client.health();
      setData(health);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch health'));
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

/**
 * Hook to get account state
 */
export function useAccount(publicKey?: string): UseQueryResult<AccountState> {
  const { client, keypair } = useZelanaContext();
  const [data, setData] = useState<AccountState | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const targetKey = publicKey || keypair?.publicKeyHex;

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    if (!targetKey) {
      setData(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const account = await client.getAccountFor(targetKey);
      setData(account);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch account'));
    } finally {
      setIsLoading(false);
    }
  }, [client, targetKey]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

/**
 * Hook to get balance (convenience wrapper around useAccount)
 */
export function useBalance(): {
  balance: bigint | null;
  isLoading: boolean;
  error: Error | null;
  refetch: () => Promise<void>;
} {
  const { data, isLoading, error, refetch } = useAccount();
  return {
    balance: data?.balance ?? null,
    isLoading,
    error,
    refetch,
  };
}

/**
 * Hook to get state roots
 */
export function useStateRoots(): UseQueryResult<StateRoots> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<StateRoots | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const roots = await client.getStateRoots();
      setData(roots);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch state roots'));
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

/**
 * Hook to get batch status
 */
export function useBatchStatus(): UseQueryResult<BatchStatusInfo> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<BatchStatusInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const status = await client.getBatchStatus();
      setData(status);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch batch status'));
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

/**
 * Hook to get global statistics
 */
export function useStats(): UseQueryResult<GlobalStats> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<GlobalStats | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const stats = await client.getStats();
      setData(stats);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch stats'));
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

// ============================================================================
// Transaction Hooks
// ============================================================================

interface TransferVariables {
  to: string | Uint8Array;
  amount: bigint;
}

/**
 * Hook for sending transfers
 */
export function useTransfer(): UseMutationResult<TransferResponse, TransferVariables> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<TransferResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const mutate = useCallback(async (variables: TransferVariables) => {
    if (!client) {
      throw new Error('Client not connected');
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await client.transfer(variables.to, variables.amount);
      setData(result);
      return result;
    } catch (e) {
      const err = e instanceof Error ? e : new Error('Transfer failed');
      setError(err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  const reset = useCallback(() => {
    setData(null);
    setError(null);
  }, []);

  return { data, isLoading, error, mutate, reset };
}

interface WithdrawVariables {
  toL1Address: string | Uint8Array;
  amount: bigint;
}

/**
 * Hook for sending withdrawals
 */
export function useWithdraw(): UseMutationResult<WithdrawResponse, WithdrawVariables> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<WithdrawResponse | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const mutate = useCallback(async (variables: WithdrawVariables) => {
    if (!client) {
      throw new Error('Client not connected');
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await client.withdraw(variables.toL1Address, variables.amount);
      setData(result);
      return result;
    } catch (e) {
      const err = e instanceof Error ? e : new Error('Withdrawal failed');
      setError(err);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, [client]);

  const reset = useCallback(() => {
    setData(null);
    setError(null);
  }, []);

  return { data, isLoading, error, mutate, reset };
}

// ============================================================================
// Transaction Tracking Hooks
// ============================================================================

/**
 * Hook to get a transaction by hash
 */
export function useTransaction(txHash: string | null): UseQueryResult<TxSummary | null> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<TxSummary | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const fetch = useCallback(async () => {
    if (!client || !txHash) {
      setData(null);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const tx = await client.getTransaction(txHash);
      setData(tx);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch transaction'));
    } finally {
      setIsLoading(false);
    }
  }, [client, txHash]);

  useEffect(() => {
    fetch();
  }, [fetch]);

  return { data, isLoading, error, refetch: fetch };
}

interface WaitForTxOptions {
  /** Target status to wait for */
  targetStatus?: 'included' | 'executed' | 'settled';
  /** Poll interval in ms */
  pollInterval?: number;
  /** Timeout in ms */
  timeout?: number;
}

/**
 * Hook to wait for a transaction to reach a status
 */
export function useWaitForTransaction(
  txHash: string | null,
  options: WaitForTxOptions = {}
): {
  tx: TxSummary | null;
  isWaiting: boolean;
  error: Error | null;
} {
  const { client } = useZelanaContext();
  const [tx, setTx] = useState<TxSummary | null>(null);
  const [isWaiting, setIsWaiting] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const {
    targetStatus = 'executed',
    pollInterval = 1000,
    timeout = 60000,
  } = options;

  useEffect(() => {
    if (!client || !txHash) {
      setTx(null);
      setIsWaiting(false);
      return;
    }

    let cancelled = false;
    setIsWaiting(true);
    setError(null);

    (async () => {
      try {
        const result = await client.waitForTransaction(
          txHash,
          targetStatus,
          timeout,
          pollInterval
        );
        if (!cancelled) {
          setTx(result);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e : new Error('Wait failed'));
        }
      } finally {
        if (!cancelled) {
          setIsWaiting(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, txHash, targetStatus, pollInterval, timeout]);

  return { tx, isWaiting, error };
}

// ============================================================================
// List Hooks
// ============================================================================

interface UseListOptions {
  limit?: number;
  autoRefresh?: boolean;
  refreshInterval?: number;
}

/**
 * Hook to list recent batches
 */
export function useBatches(options: UseListOptions = {}): UseQueryResult<BatchSummary[]> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<BatchSummary[] | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const { limit = 10, autoRefresh = false, refreshInterval = 5000 } = options;

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await client.listBatches({ limit });
      setData(result.batches);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch batches'));
    } finally {
      setIsLoading(false);
    }
  }, [client, limit]);

  useEffect(() => {
    fetch();

    if (autoRefresh) {
      intervalRef.current = setInterval(fetch, refreshInterval);
      return () => {
        if (intervalRef.current) {
          clearInterval(intervalRef.current);
        }
      };
    }
  }, [fetch, autoRefresh, refreshInterval]);

  return { data, isLoading, error, refetch: fetch };
}

/**
 * Hook to list recent transactions
 */
export function useTransactions(options: UseListOptions = {}): UseQueryResult<TxSummary[]> {
  const { client } = useZelanaContext();
  const [data, setData] = useState<TxSummary[] | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const { limit = 10, autoRefresh = false, refreshInterval = 5000 } = options;

  const fetch = useCallback(async () => {
    if (!client) {
      setError(new Error('Client not connected'));
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await client.listTransactions({ limit });
      setData(result.transactions);
    } catch (e) {
      setError(e instanceof Error ? e : new Error('Failed to fetch transactions'));
    } finally {
      setIsLoading(false);
    }
  }, [client, limit]);

  useEffect(() => {
    fetch();

    if (autoRefresh) {
      intervalRef.current = setInterval(fetch, refreshInterval);
      return () => {
        if (intervalRef.current) {
          clearInterval(intervalRef.current);
        }
      };
    }
  }, [fetch, autoRefresh, refreshInterval]);

  return { data, isLoading, error, refetch: fetch };
}
