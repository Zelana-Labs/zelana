'use client';

import { useEffect, useState } from 'react';
import HashGenerator from './HashGenerator';
import type { Node } from '../types';

interface ClusterViewProps {
  nodes: Node[];
  coordinatorRunning: boolean;
}

export default function ClusterView({ nodes, coordinatorRunning }: ClusterViewProps) {
  const [activeTab, setActiveTab] = useState<'topology' | 'details'>('topology');
  const centerX = 160;
  const centerY = 125;
  const radius = 75;

  const nodePositions = nodes.map((_, idx) => {
    const angle = (idx * 2 * Math.PI) / 5 - Math.PI / 2;
    return {
      x: centerX + radius * Math.cos(angle),
      y: centerY + radius * Math.sin(angle)
    };
  });

  return (
    <div className="p-4 h-full flex flex-col bg-bg-secondary">
      {/* Topology Window */}
      <div className="mb-6 bg-bg-primary rounded-xl border border-border shadow-sm overflow-hidden">
        {/* Window Title Bar */}
        <div className="px-4 py-2 bg-bg-secondary border-b border-border">
          <div className="flex items-center gap-2">

            <div>
              <h3 className="text-sm font-semibold text-text-primary">Network Dashboard</h3>
              <p className="text-xs text-text-tertiary">System visualization</p>
            </div>
          </div>

          {/* Simple Tabs */}
          <div className="flex relative w-full mt-2">
            <button
              onClick={() => setActiveTab('topology')}
              className={`flex-1 relative py-1.5 text-xs font-medium transition-colors text-center border-b ${activeTab === 'topology'
                  ? 'text-accent-blue border-accent-blue'
                  : 'text-text-secondary border-transparent hover:text-text-primary'
                }`}
            >
              Topology
            </button>
            <button
              onClick={() => setActiveTab('details')}
              className={`flex-1 relative py-1.5 text-xs font-medium transition-colors text-center border-b border-l border-border/30 ${activeTab === 'details'
                  ? 'text-accent-blue border-accent-blue border-l-accent-blue'
                  : 'text-text-secondary border-transparent hover:text-text-primary'
                }`}
            >
              Details
            </button>
          </div>
        </div>

        {/* Window Content */}
        <div className="p-4 bg-bg-primary min-h-[300px]">
          {activeTab === 'topology' && (
            <div className="flex justify-center">
              <svg
                width="320"
                height="270"
                viewBox="0 0 320 270"
              >
                <defs>
                  <linearGradient id="lineGradient" x1="0%" y1="0%" x2="100%" y2="100%">
                    <stop offset="0%" stopColor="#3b82f6" stopOpacity="0.6" />
                    <stop offset="100%" stopColor="#8b5cf6" stopOpacity="0.3" />
                  </linearGradient>
                </defs>

                {/* Connection Lines */}
                {nodes.map((node, idx) => {
                  if (!node.online) return null;
                  const pos = nodePositions[idx];
                  const strokeColor = node.ready ? '#059669' : node.online ? '#1e40af' : '#9ca3af';
                  const strokeWidth = node.ready ? '2' : '1.5';
                  const opacity = node.ready ? 0.8 : 0.5;

                  return (
                    <g key={`line-${idx}`}>
                      <line
                        x1={centerX}
                        y1={centerY}
                        x2={pos.x}
                        y2={pos.y}
                        stroke={strokeColor}
                        strokeWidth={strokeWidth}
                        opacity={opacity}
                        strokeLinecap="round"
                        className="transition-all duration-300"
                      />
                    </g>
                  );
                })}

                {/* Nodes */}
                {nodes.map((node, idx) => {
                  const pos = nodePositions[idx];
                  const nodeColor = node.ready ? '#059669' : node.online ? '#1e40af' : '#6b7280';
                  const statusColor = node.ready ? '#059669' : node.online ? '#d97706' : '#dc2626';

                  return (
                    <g key={`node-${idx}`} className="transition-all duration-300">
                      {/* Node circle with border */}
                      <circle
                        cx={pos.x}
                        cy={pos.y}
                        r="18"
                        fill="#1e293b"
                        stroke={nodeColor}
                        strokeWidth="2"
                        opacity={node.online ? 1 : 0.5}
                        className="transition-all duration-300 hover:stroke-width-3 hover:opacity-90"
                        style={{ filter: node.ready ? 'drop-shadow(0 0 4px rgba(5, 150, 105, 0.4))' : 'none' }}
                      />

                      {/* Professional server rack icon */}
                      <g transform={`translate(${pos.x - 7}, ${pos.y - 7})`}>
                        <rect x="2" y="2" width="10" height="7" fill="#374151" rx="1" />
                        <rect x="3" y="3" width="6" height="1" fill="#6b7280" />
                        <rect x="3" y="5" width="6" height="1" fill="#6b7280" />
                        <circle cx="5" cy="10" r="1.5" fill="#1e40af" />
                        <circle cx="8" cy="10" r="1.5" fill="#1e40af" />
                      </g>

                      {/* Node label */}
                      <text
                        x={pos.x}
                        y={pos.y + 26}
                        textAnchor="middle"
                        fill={node.online ? '#94a3b8' : '#6b7280'}
                        fontSize="11"
                        fontWeight="500"
                        className="transition-all duration-300"
                      >
                        Node {node.id}
                      </text>

                      {/* Status indicator */}
                      <circle
                        cx={pos.x + 12}
                        cy={pos.y - 12}
                        r="4.5"
                        fill={statusColor}
                        stroke="#1e293b"
                        strokeWidth="1.5"
                      />
                    </g>
                  );
                })}

                {/* Coordinator */}
                {coordinatorRunning && (
                  <circle
                    cx={centerX}
                    cy={centerY}
                    r="34"
                    fill="none"
                    stroke="#1e40af"
                    strokeWidth="1.5"
                    opacity="0.2"
                  />
                )}

                <circle
                  cx={centerX}
                  cy={centerY}
                  r="30"
                  fill="#1e293b"
                  stroke={coordinatorRunning ? '#8b5cf6' : '#6b7280'}
                  strokeWidth="2.5"
                  opacity={coordinatorRunning ? 1 : 0.5}
                  className={`transition-all duration-300 ${coordinatorRunning ? 'animate-pulse' : ''}`}
                  style={coordinatorRunning ? { animationDuration: '3s' } : {}}
                />

                {/* Professional hexagonal hub icon */}
                <g transform={`translate(${centerX - 12}, ${centerY - 12})`}>
                  <polygon
                    points="12,2 22,8 22,18 12,24 2,18 2,8"
                    fill="#374151"
                    stroke="#1e40af"
                    strokeWidth="1"
                  />
                  <circle cx="8" cy="8" r="1.5" fill="#1e40af" />
                  <circle cx="16" cy="8" r="1.5" fill="#1e40af" />
                  <circle cx="8" cy="16" r="1.5" fill="#1e40af" />
                  <circle cx="16" cy="16" r="1.5" fill="#1e40af" />
                  <circle cx="12" cy="12" r="2" fill="#1e40af" />
                </g>

                {/* Coordinator label */}
                <text
                  x={centerX}
                  y={centerY + 28}
                  textAnchor="middle"
                  fill={coordinatorRunning ? '#94a3b8' : '#6b7280'}
                  fontSize="11"
                  fontWeight="500"
                  className="transition-all duration-300"
                >
                  Coordinator
                </text>

                {/* Status indicator */}
                {coordinatorRunning && (
                  <circle
                    cx={centerX + 16}
                    cy={centerY - 16}
                    r="4.5"
                    fill="#8b5cf6"
                    stroke="#1e293b"
                    strokeWidth="1.5"
                  />
                )}
              </svg>
            </div>
          )}

          {activeTab === 'details' && (
            <div className="space-y-2">
              {/* Compact Header */}
              <div className="flex items-center gap-2 mb-3">
                <svg className="w-4 h-4 text-accent-blue" fill="currentColor" viewBox="0 0 20 20">
                  <path fillRule="evenodd" d="M3 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zm0 4a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" clipRule="evenodd" />
                </svg>
                <h4 className="text-sm font-bold text-text-primary">Network Status</h4>
              </div>

              {/* Ultra-compact Nodes List */}
              <div className="space-y-1">
                {nodes.map((node) => (
                  <div key={node.id} className="flex items-center justify-between py-1.5 px-2 bg-bg-tertiary/30 rounded">
                    <div className="flex items-center gap-2">
                      <div className={`w-2 h-2 rounded-full ${node.ready ? 'bg-accent-green' : node.online ? 'bg-accent-blue' : 'bg-text-tertiary'}`}></div>
                      <span className="text-xs font-medium text-text-primary">Node {node.id}</span>
                      <span className="text-xs text-text-tertiary truncate max-w-[120px]">{node.url}</span>
                    </div>
                    <span className={`text-xs font-medium ${node.ready ? 'text-accent-green' : node.online ? 'text-accent-blue' : 'text-text-tertiary'}`}>
                      {node.ready ? 'Ready' : node.online ? 'Online' : 'Off'}
                    </span>
                  </div>
                ))}

                {/* Simple Coordinator */}
                <div className="flex items-center justify-between py-1.5 px-2 bg-bg-tertiary/30 rounded">
                  <div className="flex items-center gap-2">
                    <div className={`w-2 h-2 rounded-full ${coordinatorRunning ? 'bg-accent-purple' : 'bg-text-tertiary'}`}></div>
                    <span className="text-xs font-medium text-text-primary">Coordinator</span>
                  </div>
                  <span className={`text-xs font-medium ${coordinatorRunning ? 'text-accent-purple' : 'text-text-tertiary'}`}>
                    {coordinatorRunning ? 'Running' : 'Stopped'}
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Stats */}
      <div className="mt-4 pt-3 border-t border-border">
        <div className="grid grid-cols-3 gap-2">
          <div className="p-2 bg-bg-tertiary rounded-lg text-center">
            <div className="text-lg font-bold text-accent-green">
              {nodes.filter(n => n.online).length}
            </div>
            <div className="text-[10px] text-text-tertiary mt-0.5 font-medium">Online</div>
          </div>
          <div className="p-2 bg-bg-tertiary rounded-lg text-center">
            <div className="text-lg font-bold text-accent-blue">
              {nodes.filter(n => n.ready).length}
            </div>
            <div className="text-[10px] text-text-tertiary mt-0.5 font-medium">Ready</div>
          </div>
          <div className="p-2 bg-bg-tertiary rounded-lg text-center">
            <div className={`text-lg font-bold ${coordinatorRunning ? 'text-accent-purple' : 'text-text-tertiary'}`}>
              {coordinatorRunning ? '●' : '○'}
            </div>
            <div className="text-[10px] text-text-tertiary mt-0.5 font-medium">Coordinator</div>
          </div>
        </div>
      </div>

      {/* Hash Generator Utility */}
      <div className="mt-4 pt-4 border-t border-border">
        <HashGenerator />
      </div>
    </div>
  );
}

