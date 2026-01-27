'use client';

import ParallelSwarmView from '../components/ParallelSwarmView';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { ArrowLeft, Home } from 'lucide-react';

interface LogEntry {
  timestamp: Date;
  message: string;
  type: 'info' | 'success' | 'error' | 'warning';
}

export default function SwarmPage() {
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const addLog = useCallback((message: string, type: 'info' | 'success' | 'error' | 'warning' = 'info') => {
    setLogs(prev => [{
      timestamp: new Date(),
      message,
      type,
    }, ...prev].slice(0, 100));
  }, []);

  return (
    <div className="min-h-screen bg-bg-primary">
      {/* Header */}
      <header className="bg-bg-secondary border-b border-border">
        <div className="px-6 py-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <Link 
                href="/"
                className="flex items-center gap-2 px-3 py-1.5 bg-bg-tertiary hover:bg-border rounded-lg text-text-secondary hover:text-text-primary transition-colors"
              >
                <ArrowLeft className="w-4 h-4" />
                <span className="text-sm">Back</span>
              </Link>
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 bg-gradient-to-br from-accent-purple to-accent-blue rounded-lg flex items-center justify-center text-xl shadow-md">
                  🐝
                </div>
                <div>
                  <h1 className="text-lg font-bold text-text-primary">
                    Parallel Swarm
                  </h1>
                  <p className="text-xs text-text-tertiary">
                    Distributed Prover Market Demo
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex">
        {/* Main Panel */}
        <div className="flex-1">
          <ParallelSwarmView onLog={addLog} />
        </div>

        {/* Log Sidebar */}
        <div className="w-96 bg-bg-secondary border-l border-border h-[calc(100vh-73px)] overflow-y-auto">
          <div className="p-4">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-semibold text-text-primary">Activity Log</h3>
              <button
                onClick={() => setLogs([])}
                className="text-xs text-text-secondary hover:text-text-primary"
              >
                Clear
              </button>
            </div>
            <div className="space-y-2">
              {logs.length === 0 ? (
                <p className="text-xs text-text-tertiary text-center py-4">
                  No activity yet. Submit a batch to get started.
                </p>
              ) : (
                logs.map((log, idx) => (
                  <div
                    key={idx}
                    className={`p-2 rounded-lg text-xs ${
                      log.type === 'success' ? 'bg-accent-green/10 text-accent-green' :
                      log.type === 'error' ? 'bg-accent-red/10 text-accent-red' :
                      log.type === 'warning' ? 'bg-accent-yellow/10 text-accent-yellow' :
                      'bg-bg-tertiary text-text-secondary'
                    }`}
                  >
                    <div className="flex items-start gap-2">
                      <span className="text-text-tertiary shrink-0">
                        {log.timestamp.toLocaleTimeString()}
                      </span>
                      <span>{log.message}</span>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
