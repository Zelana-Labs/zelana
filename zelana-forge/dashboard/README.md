# Zelana Forge Dashboard

Interactive web dashboard for the distributed ZK proving system.

## Quick Start

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Open http://localhost:5173
```

## Features

- **Cluster Management**: Start/stop Docker containers
- **Circuit Selection**: Choose between Schnorr, Hash Preimage, etc.
- **Live Logs**: Real-time logs with source filtering
- **Visual Topology**: Animated cluster visualization
- **Blind Proving**: Privacy-preserving workflow

## Architecture

```
dashboard/
├── app/
│   ├── page.tsx              # Main page
│   ├── layout.tsx            # Root layout
│   ├── globals.css           # Tailwind v4 dark theme
│   │
│   ├── components/
│   │   ├── InteractiveDashboard.tsx  # Main container
│   │   ├── ClusterView.tsx           # Node visualization
│   │   ├── WorkflowPanel.tsx         # 3-step workflow
│   │   └── LogViewer.tsx             # Log display
│   │
│   ├── circuits/             # Pluggable circuit system
│   │   ├── index.ts          # Circuit registry
│   │   ├── types.ts          # Type definitions
│   │   ├── schnorr.ts        # Schnorr circuit
│   │   └── hash-preimage.ts  # Hash preimage circuit
│   │
│   └── utils/
│       └── crypto.ts         # Client-side crypto
│
└── next.config.ts            # API proxy config
```

## Adding New Circuits

1. Create `circuits/my-circuit.ts`:

```typescript
import type { CircuitHandler } from './types';

export const myCircuit: CircuitHandler = {
  metadata: {
    id: 'my-circuit',
    name: 'My Circuit',
    icon: '🔮',
    description: 'Description here',
    statement: 'I know X such that Y',
    publicInputs: ['Public Input'],
    privateWitness: ['Private Witness'],
    useCase: 'Use case example',
    status: 'active',
  },
  setupFields: [
    { id: 'input', label: 'Input', placeholder: '...', type: 'text', isPrivate: true }
  ],
  async processSetup(inputs) {
    return { secret: inputs.input, witness: '...' };
  },
};
```

2. Register in `circuits/index.ts`:

```typescript
import { myCircuit } from './my-circuit';

export const circuitRegistry = {
  // ...existing
  'my-circuit': myCircuit,
};
```

## API Proxy

The dashboard proxies API requests:

- `/api/*` → Coordinator (port 8000)
- `/control/*` → Control server (port 9000)

Configured in `next.config.ts`.
