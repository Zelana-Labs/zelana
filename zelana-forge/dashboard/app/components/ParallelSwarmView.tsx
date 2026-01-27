'use client';

import { useState, useEffect, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { 
  Play, 
  Pause, 
  RefreshCw, 
  Cpu, 
  GitBranch, 
  CheckCircle2, 
  XCircle,
  Loader2,
  Layers,
  Zap,
  Server
} from 'lucide-react';

// ============================================================================
// Types
// ============================================================================

interface Worker {
  url: string;
  worker_id: number | null;
  ready: boolean;
  active_jobs: number;
  total_proofs: number;
  avg_proving_time_ms: number;
  last_health_check: number;
}

interface ChunkProof {
  chunk_id: number;
  worker_id: number;
  proof: string;
  public_inputs: string[];
  proving_time_ms: number;
}

interface BatchProofs {
  batch_id: string;
  proofs: ChunkProof[];
  total_time_ms: number;
  workers_used: number;
}

interface ProofSettlement {
  chunk_id: number;
  tx_signature: string;
  verified: boolean;
}

interface BatchSettlement {
  batch_id: string;
  settlements: ProofSettlement[];
  batched_tx_signature: string | null;
  settlement_time_ms: number;
  all_verified: boolean;
}

interface BatchStatus {
  batch_id: string;
  state: 'pending' | 'slicing' | 'proving' | 'settling' | 'completed' | 'failed';
  chunks_total: number;
  chunks_proved: number;
  submitted_at: number;
  proving_started_at: number | null;
  proving_completed_at: number | null;
  settled_at: number | null;
  proofs: BatchProofs | null;
  settlement: BatchSettlement | null;
  error: string | null;
}

interface Transaction {
  sender_pubkey: string;
  receiver_pubkey: string;
  amount: number;
  signature: string;
  merkle_path: string[];
}

// ============================================================================
// Demo Data Generator
// ============================================================================

function generateDemoTransactions(count: number): Transaction[] {
  return Array.from({ length: count }, (_, i) => ({
    sender_pubkey: `0x${(Math.random() * 1e16).toString(16).slice(0, 16)}`,
    receiver_pubkey: `0x${(Math.random() * 1e16).toString(16).slice(0, 16)}`,
    amount: Math.floor(Math.random() * 1000) + 1,
    signature: `0x${(Math.random() * 1e16).toString(16).slice(0, 64)}`,
    merkle_path: Array.from({ length: 32 }, () => `0x${Math.floor(Math.random() * 256).toString(16).padStart(2, '0')}`),
  }));
}

// ============================================================================
// Component
// ============================================================================

interface ParallelSwarmViewProps {
  coordinatorUrl?: string;
  onLog?: (message: string, type: 'info' | 'success' | 'error' | 'warning') => void;
}

export default function ParallelSwarmView({ 
  coordinatorUrl = 'http://localhost:8080',
  onLog 
}: ParallelSwarmViewProps) {
  const [workers, setWorkers] = useState<Worker[]>([]);
  const [currentBatch, setCurrentBatch] = useState<BatchStatus | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [txCount, setTxCount] = useState(100);
  const [chunkSize, setChunkSize] = useState(25);

  const log = useCallback((message: string, type: 'info' | 'success' | 'error' | 'warning' = 'info') => {
    console.log(`[${type.toUpperCase()}] ${message}`);
    onLog?.(message, type);
  }, [onLog]);

  // Fetch workers status
  const fetchWorkers = useCallback(async () => {
    try {
      const res = await fetch(`${coordinatorUrl}/workers`);
      const data = await res.json();
      if (data.status === 'success') {
        setWorkers(data.data.workers);
      }
    } catch (err) {
      console.error('Failed to fetch workers:', err);
    }
  }, [coordinatorUrl]);

  // Fetch batch status
  const fetchBatchStatus = useCallback(async (batchId: string) => {
    try {
      const res = await fetch(`${coordinatorUrl}/batch/${batchId}/status`);
      const data = await res.json();
      if (data.status === 'success') {
        setCurrentBatch(data.data);
        return data.data;
      }
    } catch (err) {
      console.error('Failed to fetch batch status:', err);
    }
    return null;
  }, [coordinatorUrl]);

  // Submit a new batch
  const submitBatch = async () => {
    setIsProcessing(true);
    log(`🚀 Submitting batch with ${txCount} transactions...`, 'info');

    const batchId = `batch-${Date.now()}`;
    const transactions = generateDemoTransactions(txCount);

    try {
      const res = await fetch(`${coordinatorUrl}/batch/submit`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          batch: {
            batch_id: batchId,
            initial_root: '0x0000000000000000000000000000000000000000000000000000000000000000',
            transactions,
          },
        }),
      });

      const data = await res.json();
      
      if (data.status === 'success') {
        log(`✅ Batch ${batchId} submitted: ${data.data.chunks} chunks to ${data.data.workers_assigned} workers`, 'success');
        
        // Poll for updates
        const pollInterval = setInterval(async () => {
          const status = await fetchBatchStatus(batchId);
          if (status) {
            if (status.state === 'completed') {
              log(`🎉 Batch ${batchId} completed! Settlement: ${status.settlement?.batched_tx_signature?.slice(0, 16)}...`, 'success');
              clearInterval(pollInterval);
              setIsProcessing(false);
            } else if (status.state === 'failed') {
              log(`❌ Batch ${batchId} failed: ${status.error}`, 'error');
              clearInterval(pollInterval);
              setIsProcessing(false);
            }
          }
        }, 1000);
      } else {
        log(`❌ Failed to submit batch: ${data.message}`, 'error');
        setIsProcessing(false);
      }
    } catch (err) {
      log(`❌ Error: ${err instanceof Error ? err.message : 'Unknown error'}`, 'error');
      setIsProcessing(false);
    }
  };

  // Polling
  useEffect(() => {
    fetchWorkers();
    const interval = setInterval(fetchWorkers, 5000);
    return () => clearInterval(interval);
  }, [fetchWorkers]);

  const readyWorkers = workers.filter(w => w.ready).length;
  const totalChunks = Math.ceil(txCount / chunkSize);

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-text-primary flex items-center gap-2">
            <Zap className="w-6 h-6 text-accent-purple" />
            Parallel Swarm Architecture
          </h2>
          <p className="text-text-secondary text-sm mt-1">
            Distributed proof generation with MapReduce-style parallelism
          </p>
        </div>
        <div className="flex items-center gap-3">
          <div className="px-3 py-1.5 bg-bg-tertiary rounded-lg">
            <span className="text-xs text-text-secondary">Workers Ready:</span>
            <span className="ml-2 text-sm font-bold text-accent-green">{readyWorkers}/{workers.length}</span>
          </div>
        </div>
      </div>

      {/* Configuration */}
      <div className="bg-bg-secondary border border-border rounded-xl p-5">
        <h3 className="text-sm font-semibold text-text-primary mb-4 flex items-center gap-2">
          <Layers className="w-4 h-4" />
          Batch Configuration
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div>
            <label className="block text-xs text-text-secondary mb-1">Transactions</label>
            <input
              type="number"
              value={txCount}
              onChange={(e) => setTxCount(parseInt(e.target.value) || 100)}
              className="w-full px-3 py-2 bg-bg-tertiary border border-border rounded-lg text-text-primary focus:outline-none focus:ring-2 focus:ring-accent-blue"
              min={1}
              max={10000}
            />
          </div>
          <div>
            <label className="block text-xs text-text-secondary mb-1">Chunk Size</label>
            <input
              type="number"
              value={chunkSize}
              onChange={(e) => setChunkSize(parseInt(e.target.value) || 25)}
              className="w-full px-3 py-2 bg-bg-tertiary border border-border rounded-lg text-text-primary focus:outline-none focus:ring-2 focus:ring-accent-blue"
              min={1}
              max={100}
            />
          </div>
          <div>
            <label className="block text-xs text-text-secondary mb-1">Chunks</label>
            <div className="px-3 py-2 bg-bg-tertiary border border-border rounded-lg text-text-primary">
              {totalChunks} chunks → {Math.min(totalChunks, readyWorkers)} parallel proofs
            </div>
          </div>
        </div>
        <div className="mt-4 flex justify-end">
          <button
            onClick={submitBatch}
            disabled={isProcessing || readyWorkers === 0}
            className={`flex items-center gap-2 px-5 py-2.5 rounded-lg font-medium text-sm transition-all duration-200 ${
              isProcessing || readyWorkers === 0
                ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
                : 'bg-gradient-to-r from-accent-blue to-accent-purple text-white hover:shadow-lg'
            }`}
          >
            {isProcessing ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Processing...
              </>
            ) : (
              <>
                <Play className="w-4 h-4" />
                Submit Batch
              </>
            )}
          </button>
        </div>
      </div>

      {/* Workers Grid */}
      <div className="bg-bg-secondary border border-border rounded-xl p-5">
        <h3 className="text-sm font-semibold text-text-primary mb-4 flex items-center gap-2">
          <Server className="w-4 h-4" />
          Worker Nodes
        </h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {workers.map((worker, idx) => (
            <motion.div
              key={worker.url}
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: idx * 0.1 }}
              className={`relative p-4 rounded-xl border ${
                worker.ready
                  ? 'border-accent-green/30 bg-accent-green/5'
                  : 'border-border bg-bg-tertiary'
              }`}
            >
              {/* Status indicator */}
              <div className={`absolute top-3 right-3 w-2 h-2 rounded-full ${
                worker.ready ? 'bg-accent-green animate-pulse' : 'bg-gray-500'
              }`} />
              
              <div className="flex items-center gap-2 mb-2">
                <Cpu className={`w-5 h-5 ${worker.ready ? 'text-accent-green' : 'text-gray-500'}`} />
                <span className="font-semibold text-text-primary">
                  Worker {worker.worker_id ?? idx + 1}
                </span>
              </div>
              
              <div className="space-y-1 text-xs text-text-secondary">
                <div className="flex justify-between">
                  <span>Active Jobs:</span>
                  <span className={worker.active_jobs > 0 ? 'text-accent-blue font-semibold' : ''}>
                    {worker.active_jobs}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Total Proofs:</span>
                  <span className="text-text-primary">{worker.total_proofs}</span>
                </div>
                <div className="flex justify-between">
                  <span>Avg Time:</span>
                  <span className="text-text-primary">
                    {worker.avg_proving_time_ms > 0 ? `${worker.avg_proving_time_ms}ms` : '-'}
                  </span>
                </div>
              </div>

              {/* Progress bar for active jobs */}
              {worker.active_jobs > 0 && (
                <div className="mt-3">
                  <div className="h-1 bg-bg-tertiary rounded-full overflow-hidden">
                    <motion.div
                      className="h-full bg-gradient-to-r from-accent-blue to-accent-purple"
                      initial={{ width: '0%' }}
                      animate={{ width: '100%' }}
                      transition={{ duration: 2, repeat: Infinity }}
                    />
                  </div>
                </div>
              )}
            </motion.div>
          ))}
        </div>
      </div>

      {/* Batch Status */}
      {currentBatch && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="bg-bg-secondary border border-border rounded-xl p-5"
        >
          <h3 className="text-sm font-semibold text-text-primary mb-4 flex items-center gap-2">
            <GitBranch className="w-4 h-4" />
            Batch Status: {currentBatch.batch_id}
          </h3>

          {/* Progress */}
          <div className="mb-4">
            <div className="flex justify-between text-xs text-text-secondary mb-1">
              <span>Progress</span>
              <span>{currentBatch.chunks_proved}/{currentBatch.chunks_total} chunks</span>
            </div>
            <div className="h-2 bg-bg-tertiary rounded-full overflow-hidden">
              <motion.div
                className={`h-full ${
                  currentBatch.state === 'completed' 
                    ? 'bg-accent-green' 
                    : currentBatch.state === 'failed'
                    ? 'bg-accent-red'
                    : 'bg-gradient-to-r from-accent-blue to-accent-purple'
                }`}
                initial={{ width: '0%' }}
                animate={{ 
                  width: `${(currentBatch.chunks_proved / currentBatch.chunks_total) * 100}%` 
                }}
                transition={{ duration: 0.5 }}
              />
            </div>
          </div>

          {/* State badges */}
          <div className="flex flex-wrap gap-2">
            {['pending', 'slicing', 'proving', 'settling', 'completed'].map((state) => {
              const isActive = currentBatch.state === state;
              const isPast = ['pending', 'slicing', 'proving', 'settling', 'completed'].indexOf(state) <
                            ['pending', 'slicing', 'proving', 'settling', 'completed'].indexOf(currentBatch.state);
              
              return (
                <div
                  key={state}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium ${
                    isActive
                      ? 'bg-accent-blue text-white'
                      : isPast
                      ? 'bg-accent-green/20 text-accent-green'
                      : 'bg-bg-tertiary text-text-tertiary'
                  }`}
                >
                  {isPast && <CheckCircle2 className="w-3 h-3" />}
                  {isActive && <Loader2 className="w-3 h-3 animate-spin" />}
                  {state.charAt(0).toUpperCase() + state.slice(1)}
                </div>
              );
            })}
          </div>

          {/* Settlement info */}
          {currentBatch.settlement && (
            <div className="mt-4 p-3 bg-accent-green/10 border border-accent-green/30 rounded-lg">
              <div className="flex items-center gap-2 text-accent-green font-semibold text-sm">
                <CheckCircle2 className="w-4 h-4" />
                Settled on Solana
              </div>
              <div className="mt-2 text-xs text-text-secondary">
                <div>Transaction: {currentBatch.settlement.batched_tx_signature?.slice(0, 32)}...</div>
                <div>Settlement Time: {currentBatch.settlement.settlement_time_ms}ms</div>
              </div>
            </div>
          )}

          {/* Error */}
          {currentBatch.error && (
            <div className="mt-4 p-3 bg-accent-red/10 border border-accent-red/30 rounded-lg">
              <div className="flex items-center gap-2 text-accent-red font-semibold text-sm">
                <XCircle className="w-4 h-4" />
                Error
              </div>
              <div className="mt-1 text-xs text-text-secondary">{currentBatch.error}</div>
            </div>
          )}
        </motion.div>
      )}

      {/* Architecture Diagram */}
      <div className="bg-bg-secondary border border-border rounded-xl p-5">
        <h3 className="text-sm font-semibold text-text-primary mb-4">Architecture Overview</h3>
        <div className="relative">
          <pre className="text-xs text-text-secondary font-mono overflow-x-auto">
{`                    ┌─────────────────────────────────────────────┐
                    │           COORDINATOR (Brain)               │
                    │                                             │
   Batch ──────────►│  1. Slice batch into chunks                 │
   (${txCount} txs)        │  2. Compute intermediate state roots        │
                    │  3. Dispatch chunks to workers in parallel  │
                    │  4. Collect proofs                          │
                    │  5. Submit to Solana (batched)              │
                    └─────────────────────────────────────────────┘
                              │         │         │         │
                              ▼         ▼         ▼         ▼
                         ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
                         │Worker 1│ │Worker 2│ │Worker 3│ │Worker 4│
                         │ Chunk 0│ │ Chunk 1│ │ Chunk 2│ │ Chunk 3│
                         └────────┘ └────────┘ └────────┘ └────────┘
                              │         │         │         │
                              ▼         ▼         ▼         ▼
                         ┌─────────────────────────────────────────┐
                         │              SOLANA (Verifier)          │
                         │    Batched verification of 4 proofs     │
                         └─────────────────────────────────────────┘`}
          </pre>
        </div>
      </div>
    </div>
  );
}
