export interface Node {
  id: number;
  url: string;
  online: boolean;
  ready: boolean;
}

export interface LogEntry {
  timestamp: Date;
  message: string;
  type: 'info' | 'success' | 'error' | 'warning';
  source: 'system' | 'setup' | 'prove' | 'verify';
}

export type ApiResponse<T> =
  | { status: 'success'; data: T }
  | { status: 'error'; message: string };

export interface ProofData {
  commitment: string;
  challenge: string;
  response: string;
  generator?: string;
  public_key?: string;
  witness_commitment?: WitnessCommitment;
  blind_proof?: BlindProof; // For blind proof verification
}

export interface SetupResponse {
  generator: string;
  public_key: string;
  num_nodes: number;
  threshold: number;
}

export interface WitnessCommitment {
  hash: string;
  commitment: string;
}

export interface BlindSetupResponse {
  generator: string;
  witness_commitment: WitnessCommitment;
  num_nodes: number;
  threshold: number;
  session_id: string;
}

export interface BlindProof {
  witness_commitment: WitnessCommitment;
  commitment: string;
  challenge: string;
  response: string;
}

export interface BlindProveResponse {
  blind_proof: BlindProof;
  participants: number;
}
