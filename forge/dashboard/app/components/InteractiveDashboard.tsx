'use client';

import { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import ClusterView from './ClusterView';
import WorkflowPanel from './WorkflowPanel';
import LogViewer from './LogViewer';
import type { Node, LogEntry } from '../types';

const nodeUrls = [
  '/node1',
  '/node2',
  '/node3',
  '/node4',
  '/node5',
];

export default function InteractiveDashboard() {
  const [nodesRunning, setNodesRunning] = useState(false);
  const [coordinatorRunning, setCoordinatorRunning] = useState(false);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [systemReady, setSystemReady] = useState(false);

  const addLog = useCallback((message: string, type: LogEntry['type'] = 'info', source: LogEntry['source'] = 'system') => {
    setLogs(prev => [{
      timestamp: new Date(),
      message,
      type,
      source
    }, ...prev].slice(0, 200));
  }, []);

  const checkStatus = useCallback(async () => {
    // Check coordinator
    try {
      const res = await fetch('/api/health');
      const data = await res.json();
      setCoordinatorRunning(data.status === 'success');
      setSystemReady(data.data?.ready || false);
    } catch {
      setCoordinatorRunning(false);
      setSystemReady(false);
    }

    // Check nodes
    const nodePromises = nodeUrls.map(async (url, idx) => {
      try {
        const res = await fetch(`${url}/health`);
        const data = await res.json();
        return {
          id: idx + 1,
          url: `localhost:${3001 + idx}`,
          online: true,
          ready: data.data?.ready || false
        };
      } catch {
        return {
          id: idx + 1,
          url: `localhost:${3001 + idx}`,
          online: false,
          ready: false
        };
      }
    });

    const newNodes = await Promise.all(nodePromises);
    setNodes(newNodes);
    setNodesRunning(newNodes.some(n => n.online));
  }, []);

  const startCluster = async () => {
    addLog(' Starting Docker Compose cluster...', 'info', 'system');

    try {
      const res = await fetch('/control/cluster/start', { method: 'POST' });
      const data = await res.json();

      if (data.status === 'success') {
        addLog(' Cluster started successfully!', 'success', 'system');
        addLog(' Starting 5 prover nodes...', 'info', 'system');
        addLog('⏳ Waiting for nodes to become healthy (this takes ~30 seconds)...', 'info', 'system');

        // Poll for coordinator readiness
        let attempts = 0;
        const maxAttempts = 20; // 20 attempts * 3 seconds = 60 seconds max wait

        const waitForCoordinator = async () => {
          await checkStatus();
          attempts++;

          try {
            const healthRes = await fetch('/api/health');
            const healthData = await healthRes.json();

            if (healthData.status === 'success') {
              addLog(' Coordinator is online and ready!', 'success', 'system');
              addLog(' All services are online', 'success', 'system');
            } else if (attempts < maxAttempts) {
              addLog(`⏳ Coordinator starting... (${attempts}/${maxAttempts})`, 'info', 'system');
              setTimeout(waitForCoordinator, 3000);
            } else {
              addLog('️ Coordinator took longer than expected. Check logs.', 'warning', 'system');
            }
          } catch {
            if (attempts < maxAttempts) {
              addLog(`⏳ Waiting for coordinator... (${attempts}/${maxAttempts})`, 'info', 'system');
              setTimeout(waitForCoordinator, 3000);
            } else {
              addLog(' Coordinator failed to start. Try stopping and restarting.', 'error', 'system');
            }
          }
        };

        setTimeout(waitForCoordinator, 3000);
      } else {
        addLog(` Failed to start cluster: ${data.message}`, 'error', 'system');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      addLog(` Error starting cluster: ${message}`, 'error', 'system');
      addLog('️ Make sure the control server is running (cargo run -p prover-control)', 'warning', 'system');
    }
  };

  const stopCluster = async () => {
    addLog('⏹️ Stopping Docker Compose cluster...', 'info', 'system');

    try {
      const res = await fetch('/control/cluster/stop', { method: 'POST' });
      const data = await res.json();

      if (data.status === 'success') {
        addLog(' Cluster stopped successfully', 'success', 'system');
        addLog('All containers have been removed', 'info', 'system');

        setNodesRunning(false);
        setCoordinatorRunning(false);
        setSystemReady(false);
        setNodes([]);
      } else {
        addLog(` Failed to stop cluster: ${data.message}`, 'error', 'system');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      addLog(` Error stopping cluster: ${message}`, 'error', 'system');
    }
  };

  useEffect(() => {
    addLog(' Dashboard initialized', 'success', 'system');
    addLog('Welcome to Zelana Prover - Distributed ZK Proof System', 'info', 'system');
    checkStatus();
    const interval = setInterval(checkStatus, 3000);
    return () => clearInterval(interval);
  }, [addLog, checkStatus]);

  return (
    <div className="flex flex-col h-screen bg-bg-primary">
      {/* Header */}
      <header className="bg-bg-secondary border-b border-border flex-shrink-0 shadow-sm">
        <div className="px-6 py-4">
          <div className="flex justify-between items-center">
            {/* Logo and Title */}
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 bg-gradient-to-br from-accent-blue to-accent-purple rounded-lg flex items-center justify-center text-xl shadow-md">
                
              </div>
              <div>
                <h1 className="text-lg font-bold text-text-primary">
                  Zelana Prover
                </h1>
                <p className="text-xs text-text-tertiary">
                  Distributed Zero-Knowledge Proof System
                </p>
              </div>
            </div>

            {/* System Stats */}
            <div className="flex items-center gap-3">
              {/* Parallel Swarm Link */}
              <Link
                href="/swarm"
                className="flex items-center gap-2 px-3 py-2 bg-gradient-to-r from-accent-purple/20 to-accent-blue/20 hover:from-accent-purple/30 hover:to-accent-blue/30 border border-accent-purple/30 rounded-lg transition-all duration-200"
              >
                <span className="text-lg"></span>
                <span className="text-xs font-semibold text-text-primary">Parallel Swarm</span>
              </Link>

              <div className="flex items-center gap-2 px-3 py-2 bg-bg-tertiary rounded-lg">
                <div className="text-xs text-text-secondary">Coordinator:</div>
                <div className={`flex items-center gap-1.5 ${coordinatorRunning ? 'text-accent-green' : 'text-accent-red'}`}>
                  <div className={`w-2 h-2 rounded-full ${coordinatorRunning ? 'bg-accent-green animate-pulse' : 'bg-accent-red'}`}></div>
                  <span className="text-xs font-semibold">{coordinatorRunning ? 'Online' : 'Offline'}</span>
                </div>
              </div>

              <div className="flex items-center gap-2 px-3 py-2 bg-bg-tertiary rounded-lg">
                <div className="text-xs text-text-secondary">Nodes:</div>
                <div className="flex items-center gap-1.5">
                  <span className="text-xs font-semibold text-accent-green">{nodes.filter(n => n.ready).length}</span>
                  <span className="text-xs text-text-tertiary">/</span>
                  <span className="text-xs font-semibold text-text-primary">{nodes.length}</span>
                  <span className="text-xs text-text-tertiary">Ready</span>
                </div>
              </div>

              {/* Action button */}
              {!coordinatorRunning || !nodesRunning ? (
                <button
                  onClick={startCluster}
                  className="flex items-center gap-2 px-4 py-2 bg-accent-green hover:bg-accent-green/90 rounded-lg font-medium text-sm text-white transition-all duration-200 shadow-md hover:shadow-lg"
                >
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path d="M6.3 2.841A1.5 1.5 0 004 4.11V15.89a1.5 1.5 0 002.3 1.269l9.344-5.89a1.5 1.5 0 000-2.538L6.3 2.84z" />
                  </svg>
                  Start Cluster
                </button>
              ) : (
                <button
                  onClick={stopCluster}
                  className="flex items-center gap-2 px-4 py-2 bg-accent-red hover:bg-accent-red/90 rounded-lg font-medium text-sm text-white transition-all duration-200 shadow-md hover:shadow-lg"
                >
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8 7a1 1 0 00-1 1v4a1 1 0 001 1h4a1 1 0 001-1V8a1 1 0 00-1-1H8z" clipRule="evenodd" />
                  </svg>
                  Stop Cluster
                </button>
              )}
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <div className="flex-1 overflow-hidden flex flex-col lg:flex-row gap-0">
        {/* Left Sidebar - Cluster View */}
        <div className="w-full lg:w-80 xl:w-96 bg-bg-secondary border-b lg:border-b-0 lg:border-r border-border overflow-y-auto flex-shrink-0">
          <ClusterView nodes={nodes} coordinatorRunning={coordinatorRunning} />
        </div>

        {/* Center Content - Workflow */}
        <div className="flex-1 bg-bg-primary overflow-y-auto">
          <WorkflowPanel
            coordinatorRunning={coordinatorRunning}
            onLog={addLog}
          />
        </div>

        {/* Right Sidebar - Logs */}
        <div className="w-full lg:w-96 xl:w-[440px] bg-bg-secondary border-t lg:border-t-0 lg:border-l border-border overflow-y-auto flex-shrink-0">
          <LogViewer logs={logs} nodes={nodes} onClearLogs={() => setLogs([])} />
        </div>
      </div>
    </div>
  );
}
