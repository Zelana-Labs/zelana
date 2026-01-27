'use client';

import { useState, useRef, useEffect } from 'react';
import type { LogEntry, Node } from '../types';

interface LogViewerProps {
  logs: LogEntry[];
  nodes: Node[];
  onClearLogs: () => void;
}

export default function LogViewer({ logs, nodes, onClearLogs }: LogViewerProps) {
  const [selectedSource, setSelectedSource] = useState<'all' | LogEntry['source']>('all');
  const [selectedContainer, setSelectedContainer] = useState<string>('coordinator');
  const [containerLogs, setContainerLogs] = useState('');
  const [fetchingLogs, setFetchingLogs] = useState(false);
  const [showModal, setShowModal] = useState(false);
  const logContainerRef = useRef<HTMLDivElement>(null);

  const filteredLogs = selectedSource === 'all'
    ? logs
    : logs.filter(log => log.source === selectedSource);

  // Auto-fetch logs when selected container changes (reduced frequency to prevent glitching)
  useEffect(() => {
    if (selectedContainer) {
      fetchContainerLogs(selectedContainer);
      const interval = setInterval(() => {
        fetchContainerLogs(selectedContainer);
      }, 10000); // Refresh every 10 seconds (reduced from 5 to prevent glitching)
      return () => clearInterval(interval);
    }
  }, [selectedContainer]);

  const getLogIcon = (type: LogEntry['type']) => {
    switch (type) {
      case 'success': return '✓';
      case 'error': return '✗';
      case 'warning': return '⚠';
      default: return 'ℹ';
    }
  };

  const getLogColorClasses = (type: LogEntry['type']) => {
    switch (type) {
      case 'success': return 'bg-accent-green/10 border-accent-green/30 text-accent-green';
      case 'error': return 'bg-accent-red/10 border-accent-red/30 text-accent-red';
      case 'warning': return 'bg-accent-yellow/10 border-accent-yellow/30 text-accent-yellow';
      default: return 'bg-accent-blue/10 border-accent-blue/30 text-accent-blue';
    }
  };

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString('en-US', {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    });
  };

  const parseContainerLogs = (rawLogs: string) => {
    const lines = rawLogs.split('\n').filter(line => line.trim());
    return lines.map((line, idx) => {
      // Try to parse timestamp and remove ANSI color codes
      const cleanLine = line.replace(/\x1b\[[0-9;]*m/g, '');

      // Match common log patterns
      const timestampMatch = cleanLine.match(/^\[?(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})/);
      const levelMatch = cleanLine.match(/\b(INFO|ERROR|WARN|DEBUG|TRACE)\b/i);

      const timestamp = timestampMatch ? timestampMatch[1] : '';
      const level = levelMatch ? levelMatch[1].toUpperCase() : 'INFO';

      // Extract message part after level
      let message = cleanLine;
      if (levelMatch && levelMatch.index !== undefined) {
        message = cleanLine.substring(levelMatch.index + levelMatch[0].length).trim();
        // Remove common prefixes like "prover_coordinator:"
        message = message.replace(/^[^:]+:\s*/, '');
      }

      let icon = 'ℹ';
      let colorClass = 'text-accent-blue';

      if (level === 'ERROR') {
        icon = '✗';
        colorClass = 'text-accent-red';
      } else if (level === 'WARN') {
        icon = '⚠';
        colorClass = 'text-accent-yellow';
      } else if (level === 'INFO') {
        icon = '✓';
        colorClass = 'text-accent-green';
      }

      return { line: message, timestamp, level, icon, colorClass, key: idx };
    });
  };

  const fetchContainerLogs = async (container: string) => {
    if (fetchingLogs) return;

    setSelectedContainer(container);
    setFetchingLogs(true);
    setContainerLogs('Loading logs...');

    try {
      const res = await fetch(`/control/cluster/logs/${container}`);
      const data = await res.json();

      if (data.status === 'success') {
        setContainerLogs(data.data.logs || 'No logs available');
      } else {
        setContainerLogs(`Error: ${data.message}`);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      setContainerLogs(`Error fetching logs: ${message}`);
    } finally {
      setFetchingLogs(false);
    }
  };

  const restartContainer = async (container: string) => {
    try {
      const res = await fetch(`/control/cluster/restart/${container}`, {
        method: 'POST'
      });
      const data = await res.json();

      if (data.status === 'success') {
        alert(`Container ${container} restarted successfully`);
      } else {
        alert(`Failed to restart: ${data.message}`);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      alert(`Error: ${message}`);
    }
  };

  const parsedContainerLogs = containerLogs ? parseContainerLogs(containerLogs) : [];

  return (
    <div className="p-4 h-full flex flex-col bg-bg-secondary">
      {/* Header */}
      <div className="flex justify-between items-center mb-4">
        <div>
          <h2 className="text-lg font-bold text-text-primary">Activity Logs</h2>
          <p className="text-xs text-text-tertiary mt-0.5">Real-time system events</p>
        </div>
        <button
          onClick={onClearLogs}
          className="px-3 py-2 text-xs bg-bg-tertiary hover:bg-accent-red hover:text-white rounded-lg font-medium transition-all duration-200"
        >
          Clear Logs
        </button>
      </div>

      {/* Source Filter */}
      <div className="flex flex-wrap gap-2 mb-4">
        {(['all', 'system', 'setup', 'prove', 'verify'] as const).map((source) => (
          <button
            key={source}
            onClick={() => setSelectedSource(source)}
            className={`px-3 py-1.5 text-xs rounded-lg font-medium transition-all duration-200 ${
              selectedSource === source
                ? 'bg-accent-blue text-white shadow-lg'
                : 'bg-bg-tertiary text-text-secondary hover:bg-bg-primary'
            }`}
          >
            {source.charAt(0).toUpperCase() + source.slice(1)}
            {source === 'all' && <span className="ml-1 opacity-60">({logs.length})</span>}
          </button>
        ))}
      </div>

      {/* Logs */}
      <div
        ref={logContainerRef}
        className="flex-1 overflow-y-auto bg-bg-primary rounded-lg p-2 space-y-1 mb-4"
      >
        {filteredLogs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-text-tertiary text-center p-10">
            <div className="text-4xl mb-3 opacity-50">📋</div>
            <p className="text-sm">No activity logs yet</p>
            <p className="text-xs mt-1 opacity-70">Start the workflow to see events here</p>
          </div>
        ) : (
          filteredLogs.map((log, idx) => (
            <div
              key={`${log.timestamp.getTime()}-${idx}`}
              className={`flex items-start gap-2 p-2 rounded-lg border transition-all duration-200 ${getLogColorClasses(log.type)}`}
            >
              <span className="text-base mt-0.5">{getLogIcon(log.type)}</span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-[10px] font-mono text-text-tertiary">{formatTime(log.timestamp)}</span>
                  <span className="text-[10px] font-semibold uppercase px-1.5 py-0.5 bg-bg-secondary rounded">
                    {log.source}
                  </span>
                </div>
                <div className="text-xs text-text-primary break-words leading-relaxed">{log.message}</div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Docker Containers & Logs - Combined Card */}
    
        <div className="flex justify-between items-center mb-3">
          <h3 className="text-sm font-semibold text-text-primary">🐳 Container Logs</h3>
          <div className="flex gap-2">
            <button
              onClick={() => fetchContainerLogs(selectedContainer)}
              disabled={fetchingLogs}
              className="px-3 py-1.5 text-xs bg-accent-blue hover:bg-accent-blue/80 disabled:opacity-50 rounded-lg font-medium transition-all duration-200"
            >
              {fetchingLogs ? '⏳' : '🔄'}
            </button>
            <button
              onClick={() => setShowModal(true)}
              className="px-3 py-1.5 text-xs bg-bg-primary hover:bg-accent-purple/20 border border-border rounded-lg font-medium transition-all duration-200"
            >
              ⛶
            </button>
            <button
              onClick={() => restartContainer(selectedContainer)}
              className="px-3 py-1.5 text-xs bg-accent-red hover:bg-accent-red/80 rounded-lg font-medium transition-all duration-200"
            >
              ⚡
            </button>
          </div>
        </div>

        {/* Container Selection */}
        <div className="flex flex-wrap gap-2 mb-4">
          {(['coordinator', 'node1', 'node2', 'node3', 'node4', 'node5'] as const).map((container) => (
            <button
              key={container}
              onClick={() => setSelectedContainer(container)}
              className={`px-1 py-1.5 text-xs rounded-lg font-medium transition-all duration-200 ${
                selectedContainer === container
                  ? 'bg-accent-blue text-white shadow-lg'
                  : 'bg-bg-tertiary text-text-secondary hover:bg-bg-primary'
              }`}
            >
              {container === 'coordinator' ? 'Coordinator' : `Node ${container.slice(4)}`}
            </button>
          ))}
        </div>

        {/* Logs Display */}
        <div className="bg-bg-primary rounded-lg p-3 h-80 overflow-y-auto">
          {fetchingLogs ? (
            <div className="flex items-center justify-center h-full text-text-secondary">
              <div className="text-center">
                <div className="text-2xl mb-2">⏳</div>
                <p className="text-xs">Loading logs...</p>
              </div>
            </div>
          ) : parsedContainerLogs.length === 0 ? (
            <div className="flex items-center justify-center h-full text-text-tertiary">
              <div className="text-center">
                <div className="text-2xl mb-2 opacity-50">📄</div>
                <p className="text-xs">No logs available</p>
              </div>
            </div>
          ) : (
            <div className="space-y-0.5 font-mono text-xs">
              {parsedContainerLogs.slice(0, 50).map((logLine) => (
                <div
                  key={`${selectedContainer}-${logLine.key}`}
                  className="flex gap-2 p-1.5 rounded hover:bg-bg-secondary transition-colors"
                >
                  <span className={`${logLine.colorClass} flex-shrink-0`}>{logLine.icon}</span>
                  {logLine.timestamp && (
                    <span className="text-text-tertiary flex-shrink-0 text-[10px]">
                      {logLine.timestamp.slice(11, 19)}
                    </span>
                  )}
                  <span className={`${logLine.colorClass} font-semibold flex-shrink-0 text-[10px]`}>
                    {logLine.level}
                  </span>
                  <span className="text-text-primary break-all leading-relaxed text-[11px]">
                    {logLine.line.replace(/^\[?(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[^\]]*)\]?\s*/, '').replace(/\[32m|\[0m|\[2m|\[31m/g, '')}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
   

      {/* Container Log Modal */}
      {showModal && (
        <div
          className="fixed inset-0 bg-black/90 backdrop-blur-sm flex items-center justify-center z-50 p-4"
          onClick={() => setShowModal(false)}
        >
          <div
            className="bg-bg-secondary border border-border rounded-2xl w-full max-w-6xl max-h-[90vh] flex flex-col shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex justify-between items-center p-6 border-b border-border">
              <div>
                <h3 className="text-lg font-bold text-text-primary">
                  {selectedContainer === 'coordinator' ? '🎯' : '🖥️'} {selectedContainer} Container Logs
                </h3>
                <p className="text-xs text-text-tertiary mt-1">Last 100 lines</p>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => fetchContainerLogs(selectedContainer)}
                  disabled={fetchingLogs}
                  className="px-4 py-2 text-xs bg-accent-blue hover:bg-accent-blue/80 disabled:opacity-50 rounded-lg font-medium transition-all duration-200"
                >
                  {fetchingLogs ? '⏳ Loading...' : '🔄 Refresh'}
                </button>
                <button
                  onClick={() => restartContainer(selectedContainer)}
                  className="px-4 py-2 text-xs bg-accent-red hover:bg-accent-red/80 rounded-lg font-medium transition-all duration-200"
                >
                  ⚡ Restart
                </button>
                <button
                  onClick={() => setShowModal(false)}
                  className="px-4 py-2 text-xs bg-bg-tertiary hover:bg-bg-primary rounded-lg font-medium transition-all duration-200"
                >
                  ✕ Close
                </button>
              </div>
            </div>

            {/* Modal Content */}
            <div className="flex-1 overflow-y-auto p-4 bg-bg-primary m-4 rounded-lg">
              {fetchingLogs ? (
                <div className="flex items-center justify-center h-full text-text-secondary">
                  <div className="text-center">
                    <div className="text-4xl mb-3">⏳</div>
                    <p>Loading logs...</p>
                  </div>
                </div>
              ) : parsedContainerLogs.length === 0 ? (
                <div className="flex items-center justify-center h-full text-text-tertiary">
                  <div className="text-center">
                    <div className="text-4xl mb-3 opacity-50">📄</div>
                    <p>No logs available</p>
                  </div>
                </div>
              ) : (
                <div className="space-y-0.5 font-mono text-xs">
                  {parsedContainerLogs.map((logLine) => (
                    <div
                      key={logLine.key}
                      className="flex gap-2 p-2 rounded hover:bg-bg-secondary transition-colors"
                    >
                      <span className={`${logLine.colorClass} flex-shrink-0`}>{logLine.icon}</span>
                      {logLine.timestamp && (
                        <span className="text-text-tertiary flex-shrink-0 text-[10px]">
                          {logLine.timestamp.slice(11, 19)}
                        </span>
                      )}
                      <span className={`${logLine.colorClass} font-semibold flex-shrink-0 text-[10px]`}>
                        {logLine.level}
                      </span>
                      <span className="text-text-primary break-all leading-relaxed">
                        {logLine.line.replace(/^\[?(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[^\]]*)\]?\s*/, '').replace(/\[32m|\[0m|\[2m|\[31m/g, '')}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
