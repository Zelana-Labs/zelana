# AGENTS.md - Development Guidelines for Zelana Forge

This document provides development guidelines for agentic coding assistants working on the Zelana Forge distributed zero-knowledge proof system.

## Project Overview

Zelana Forge is a distributed zero-knowledge proof system enabling mobile users to submit transactions without revealing sensitive data. The system uses blind proving where mobile devices generate commitments and the network handles heavy cryptographic computation.

**Key components:**
- **Rust crates**: Core crypto, networking, nodes, coordinator
- **Next.js dashboard**: TypeScript/React interactive UI
- **Deployment configs**: Docker, Kubernetes

## Build, Lint, and Test Commands

### Rust (Primary Language)

```bash
# Build all crates
cargo build --workspace

# Build optimized release
cargo build --release --workspace

# Run all tests
cargo test --workspace

# Run single test
cargo test test_name
# Or run tests in specific crate:
cargo test -p prover-core
# Or run tests matching pattern:
cargo test -- test_pattern

# Format code
cargo fmt --all

# Lint code
cargo clippy --all-targets --all-features -- -D warnings

# Check compilation without building
cargo check --workspace
```

### TypeScript/JavaScript (Dashboard)

```bash
cd dashboard

# Install dependencies
npm install

# Development server
npm run dev

# Build for production
npm run build

# Lint code
npm run lint
```

### Integration Testing

```bash
# Local cluster test
./scripts/test-local.sh

# Docker integration test
./scripts/test.sh
```

## Code Style Guidelines

### Rust Conventions

#### Imports and Dependencies
```rust
// Group imports: std, external crates, internal crates
use std::collections::HashMap;
use ark_bn254::Fr;
use serde::{Deserialize, Serialize};
use prover_core::{shamir, schnorr, ShareSet};

// Use workspace dependencies consistently
// See Cargo.toml for available workspace deps
```

#### Naming Conventions
- **Functions/Methods**: `snake_case`
- **Types/Structs/Enums**: `PascalCase`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **Modules**: `snake_case`
- **Fields**: `snake_case`

#### Documentation
```rust
//! # Module Level Documentation
//!
//! Describe the module's purpose and provide examples.

// Function documentation with parameter descriptions
/// # Arguments
/// * `param` - Description of parameter
///
/// # Returns
/// Description of return value
///
/// # Errors
/// Description of possible errors
pub fn example_function(param: Type) -> Result<Type, Error> {
    // Implementation
}
```

#### Error Handling
```rust
// Use thiserror for custom error types
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProverError {
    #[error("Insufficient shares: need at least {threshold}, got {provided}")]
    InsufficientShares { threshold: usize, provided: usize },

    #[error("Verification failed")]
    VerificationFailed,
}

// Use Result<T, ProverError> for fallible operations
pub type Result<T> = std::result::Result<T, ProverError>;
```

#### Type Safety and Security
- Use `ark_bn254::Fr` for field elements (consistent with workspace)
- Prefer `&[u8]` over `Vec<u8>` for inputs when possible
- Use `getrandom::getrandom` for entropy (not `rand::thread_rng()`)
- Validate all inputs, especially cryptographic parameters
- Never expose secrets in logs or error messages

#### Testing
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::test_rng;

    #[test]
    fn test_example() {
        let mut rng = test_rng();
        // Test implementation
        assert!(result.is_ok());
    }
}
```

### TypeScript/React Conventions

#### File Structure
```
dashboard/
├── app/
│   ├── components/     # React components (.tsx)
│   ├── circuits/       # Circuit definitions (.ts)
│   ├── utils/          # Utilities (.ts)
│   └── types.ts        # Type definitions
```

#### Component Patterns
```tsx
'use client';

import { useState, useEffect, useCallback } from 'react';
import type { Node } from '../types';

// Functional components with hooks
export default function ComponentName({ prop }: Props) {
  const [state, setState] = useState<InitialType>(initialValue);

  // Use useCallback for event handlers
  const handleEvent = useCallback(() => {
    // Implementation
  }, [dependencies]);

  return (
    <div>
      {/* JSX content */}
    </div>
  );
}
```

#### Type Definitions
```typescript
// Use interfaces for objects
export interface Node {
  id: number;
  url: string;
  online: boolean;
  ready: boolean;
}

// Use union types for discriminated unions
export type ApiResponse<T> =
  | { status: 'success'; data: T }
  | { status: 'error'; message: string };
```

#### Naming Conventions
- **Components**: `PascalCase`
- **Functions/Variables**: `camelCase`
- **Types/Interfaces**: `PascalCase`
- **Files**: `camelCase.tsx` or `camelCase.ts`
- **Constants**: `camelCase` or `UPPER_CASE`

#### Styling
- Uses **Tailwind CSS** for styling
- Consistent with Tailwind utility classes
- Responsive design patterns

## Security Best Practices

### Cryptographic Security
- **Never log secrets** or private keys
- **Validate all inputs** before cryptographic operations
- **Use constant-time operations** where timing attacks are possible
- **Proper entropy sources** (getrandom, not rand::thread_rng)
- **Secure key generation** and handling

### Network Security
- **Input validation** on all API endpoints
- **Proper error messages** (no information leakage)
- **Rate limiting** considerations
- **CORS configuration** for web endpoints

## Development Workflow

### Before Committing
1. **Format code**: `cargo fmt --all`
2. **Lint code**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Run tests**: `cargo test --workspace`
4. **TypeScript**: `cd dashboard && npm run lint && npm run build`

### Commit Messages
- **Format**: `type(scope): description`
- **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`
- **Examples**:
  - `feat(schnorr): add blind proving capability`
  - `fix(setup): handle duplicate node indices`

## Code Locations
- **Core crypto**: `crates/prover-core/src/`
- **Network types**: `crates/prover-network/src/`
- **Node server**: `crates/prover-node/src/`
- **Coordinator**: `crates/prover-coordinator/src/`
- **Dashboard UI**: `dashboard/app/`

## Architecture Patterns

### Component Communication
- **Coordinator**: Orchestrates distributed proving
- **Nodes**: Hold secret shares, perform computations
- **Dashboard**: User interface and control
- **Message passing**: Typed messages between components

### Circuit Abstraction
- **Pluggable circuits**: Easy to add new ZK statements
- **Consistent interface**: Setup, prove, verify pattern
- **Type safety**: Strongly typed circuit configurations

This document should be updated as the codebase evolves. When making changes that affect these guidelines, update this file accordingly.</content>
<parameter name="filePath">/home/bheet/projects/zelana/zelana-forge/AGENTS.md