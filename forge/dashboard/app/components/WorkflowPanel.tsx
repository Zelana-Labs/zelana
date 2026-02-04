'use client';

import { useState, useEffect } from 'react';
import type { LogEntry, ApiResponse, SetupResponse, BlindSetupResponse, ProofData, BlindProveResponse } from '../types';
import { commitToWitness, generateSalt } from '../utils/crypto';
import {
  getAllCircuits,
  getCircuitHandler,
  getCircuitMetadata,
  isCircuitActive,
  type CircuitInputField,
  type CircuitSetupInputs,
} from '../circuits';

// Utility function to convert hex string to BigInt
const hexToBigInt = (hex: string): bigint => {
  return BigInt('0x' + hex);
};

interface WorkflowPanelProps {
  coordinatorRunning: boolean;
  onLog: (message: string, type: LogEntry['type'], source: LogEntry['source']) => void;
}

interface ShareInfo {
  node_id: number;
  share_index: number;
}

const steps = [
  { id: 0, title: 'Setup', icon: '', description: 'Initialize blind proving session' },
  { id: 1, title: 'Get Proof', icon: '', description: 'Generate distributed proof' },
  { id: 2, title: 'Verify', icon: '', description: 'Validate the proof' },
];

interface RequestResponse {
  request: unknown;
  response: unknown;
  timestamp: Date;
}

export default function WorkflowPanel({ coordinatorRunning, onLog }: WorkflowPanelProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [completedSteps, setCompletedSteps] = useState<number[]>([]);
  const [loading, setLoading] = useState<number | null>(null);
  const [setupData, setSetupData] = useState<SetupResponse | null>(null);
  const [proofData, setProofData] = useState<ProofData | null>(null);
  const [verifyResult, setVerifyResult] = useState<boolean | null>(null);
  const [setupPreview, setSetupPreview] = useState<any>(null);
  const [previewSalt, setPreviewSalt] = useState<string>('');
  const [previewHash, setPreviewHash] = useState<string>('');
  const [verifyOverrides, setVerifyOverrides] = useState<{witness?: string, salt?: string}>({});

  const [circuitInputs, setCircuitInputs] = useState<CircuitSetupInputs>({});
  const [selectedCircuit, setSelectedCircuit] = useState<string>('schnorr');

  const [circuitExtraData, setCircuitExtraData] = useState<Record<string, unknown>>({});

  const [salt, setSalt] = useState<Uint8Array | null>(null);
  const [sessionId, setSessionId] = useState<string>('');


  const [shares, setShares] = useState<ShareInfo[]>([]);
  const [publicWitness, setPublicWitness] = useState<string>('');

  const [requestResponses, setRequestResponses] = useState<Record<number, RequestResponse>>({});



  const circuits = getAllCircuits();


  const currentHandler = getCircuitHandler(selectedCircuit);
  const currentMetadata = getCircuitMetadata(selectedCircuit);

  const handleInputChange = (fieldId: string, value: string) => {
    setCircuitInputs(prev => ({ ...prev, [fieldId]: value }));
  };

  const handleCircuitChange = (circuitId: string) => {
    setSelectedCircuit(circuitId);
    // Reset all circuit-specific state when switching circuits
    setCircuitInputs({});
    setCircuitExtraData({});
    setSetupData(null);
    setProofData(null);
    setSessionId('');
    setCompletedSteps([]);
    setVerifyResult(null);
    setSetupPreview(null);
    setPreviewSalt('');
    setPreviewHash('');
    setSalt(null);
    setPublicWitness('');
    setRequestResponses({}); // Reset API request/response history
    setVerifyOverrides({}); // Reset verify override values
  };



  // Auto-populate defaults and compute setup preview when inputs change
  useEffect(() => {
    const handler = getCircuitHandler(selectedCircuit);
    if (handler) {
      // Auto-populate defaults for empty fields
      const defaults: CircuitSetupInputs = {};
      let hasChanges = false;
      handler.setupFields.forEach(field => {
        if (field.defaultValue && (!circuitInputs[field.id] || circuitInputs[field.id].trim() === '')) {
          defaults[field.id] = field.defaultValue;
          hasChanges = true;
        }
      });
      if (hasChanges) {
        setCircuitInputs(prev => ({ ...prev, ...defaults }));
        return; // Exit early, the effect will run again with new inputs
      }

      // Compute setup preview
      const computeSetupPreview = async () => {
        try {
          const setupPayload = await handler.processSetup(circuitInputs);
          // Generate a preview salt and compute the commitment hash
          const previewSalt = generateSalt();
          const commitmentHash = await commitToWitness(setupPayload.witness, previewSalt);
          const saltHex = Array.from(previewSalt).map(b => b.toString(16).padStart(2, '0')).join('');

          setSetupPreview({
            circuit_type: selectedCircuit,
            witness_commitment: {
              hash: commitmentHash
            },
            secret: setupPayload.secret.substring(0, 20) + (setupPayload.secret.length > 20 ? '...' : ''),
          });
          // Store the preview salt and hash separately (not in API request)
          setPreviewSalt(saltHex);
          setPreviewHash(commitmentHash);
        } catch (error) {
          setSetupPreview({
            circuit_type: selectedCircuit,
            witness_commitment: {
              hash: 'error computing preview'
            },
            secret: 'error computing preview'
          });
          setPreviewSalt('error computing preview');
          setPreviewHash('error computing preview');
        }
      };

      computeSetupPreview();
    } else {
      setSetupPreview({
        circuit_type: selectedCircuit,
        witness_commitment: {
          hash: 'circuit not available'
        },
        secret: 'circuit not available'
      });
      setPreviewSalt('circuit not available');
      setPreviewHash('circuit not available');
    }
  }, [selectedCircuit, circuitInputs]);

  const performSetup = async () => {
    if (!coordinatorRunning) {
      throw new Error('Coordinator is not running. Start the cluster first.');
    }

    if (!currentHandler || !isCircuitActive(selectedCircuit)) {
      throw new Error('Selected circuit is not available.');
    }

    const setupPayload = await currentHandler.processSetup(circuitInputs);
    const { secret, witness } = setupPayload;

    console.log('Setup: circuitInputs =', circuitInputs);
    console.log('Setup: setupPayload =', setupPayload);
    console.log('Setup: secret =', secret);
    console.log('Setup: witness =', witness);

    // The witness for commitment comes from the circuit's processSetup
    // This is the actual data that will be verified later (e.g., message for Schnorr)
    const actualWitness = witness;
    console.log('Setup: using private input as witness =', actualWitness);

    // Verify we have a valid witness
    if (!actualWitness || actualWitness.trim() === '') {
      throw new Error('Setup failed: Private witness input is empty');
    }

    // Cache the witness for verification
    sessionStorage.setItem('zelana_witness', actualWitness);
    setPublicWitness(actualWitness);
    console.log('Setup: witness cached for verification');

    const saltBytes = generateSalt();
    console.log('Setup: saltBytes length =', saltBytes.length);
    const commitmentHash = await commitToWitness(witness, saltBytes);

    setSalt(saltBytes);
    console.log('Setup: completed successfully');

    const requestPayload = {
      circuit_type: selectedCircuit,
      witness_commitment: { hash: commitmentHash },
      secret
    };

    const res = await fetch('/api/setup', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(requestPayload)
    });

    if (!res.ok) {
      const text = await res.text();
      throw new Error(`HTTP ${res.status}: ${text}`);
    }

    const data: ApiResponse<BlindSetupResponse> = await res.json();

    // Store API details for step 0 (Setup)
    setRequestResponses(prev => ({
      ...prev,
      0: { request: requestPayload, response: data, timestamp: new Date() }
    }));

    if (data.status === 'success') {
      setSetupData({
        generator: data.data.generator,
        public_key: '', // Not returned in blind setup
        num_nodes: data.data.num_nodes,
        threshold: data.data.threshold
      });
      if (data.data.session_id) {
        setSessionId(data.data.session_id);
      }
      // Shares are not returned in blind setup response
      setShares([
        { node_id: 1, share_index: 1 },
        { node_id: 2, share_index: 2 },
        { node_id: 3, share_index: 3 },
      ]);
      setCompletedSteps(prev => [...prev, 0]);
      onLog(` Setup complete: ${data.data.num_nodes} nodes, threshold ${data.data.threshold}`, 'success', 'setup');
      return data.data;
    } else {
      throw new Error(data.message || 'Unknown error');
    }
  };



  const handleSetup = async () => {
    setLoading(0);
    onLog(' Setting up blind proving session...', 'info', 'setup');

    try {
      // Use the performSetup logic which properly handles witness extraction
      const result = await performSetup();
      return result;
    } catch (err) {
      onLog(` Setup failed: ${err instanceof Error ? err.message : 'Unknown error'}`, 'error', 'setup');
      throw err;
    } finally {
      setLoading(null);
    }
  };

  const handleGetProof = async () => {
    if (!completedSteps.includes(0)) {
      onLog(' Please run Setup first before getting proof.', 'error', 'prove');
      return;
    }

    if (!sessionId) {
      onLog(' No session ID found. Please run Setup first.', 'error', 'prove');
      return;
    }

    setLoading(1);
    onLog(` Generating distributed proof for session: ${sessionId}...`, 'info', 'prove');

    try {
      const requestPayload = { session_id: sessionId };

      const res = await fetch('/api/prove', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestPayload)
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(`HTTP ${res.status}: ${text}`);
      }

       const data: ApiResponse<BlindProveResponse> = await res.json();

       setRequestResponses(prev => ({
         ...prev,
         1: { request: requestPayload, response: data, timestamp: new Date() }
       }));

       if (data.status === 'success') {
        const blindProof = data.data.blind_proof;
        setProofData({
          commitment: blindProof.commitment,
          challenge: blindProof.challenge,
          response: blindProof.response,
          witness_commitment: blindProof.witness_commitment,
          blind_proof: blindProof // Store the full blind proof for verification
        }); // Store the proof data

        setCompletedSteps(prev => [...prev, 1]);
        setCurrentStep(2);
        onLog(` Proof generated with ${data.data.participants || 'threshold'} nodes`, 'success', 'prove');
      } else {
        onLog(` Proof generation failed: ${data.message}`, 'error', 'prove');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      onLog(` Proof generation error: ${message}`, 'error', 'prove');
    } finally {
      setLoading(null);
    }
  };

  const handleVerify = async () => {
    console.log('=== VERIFY DEBUG ===');
    console.log('Verify: proofData exists =', !!proofData);
    console.log('Verify: salt exists =', !!salt, 'length =', salt?.length);
    console.log('Verify: publicWitness exists =', !!publicWitness, 'value =', publicWitness);
    console.log('Verify: sessionStorage witness =', sessionStorage.getItem('zelana_witness'));
    console.log('Verify: circuitInputs =', circuitInputs);
    console.log('Verify: selectedCircuit =', selectedCircuit);

    if (!proofData) {
      onLog('️ Please generate a proof first', 'warning', 'verify');
      return;
    }

    // Get the witness - use override if provided, otherwise use setup witness
    let witnessToUse = verifyOverrides.witness !== undefined ? verifyOverrides.witness :
                      (publicWitness || sessionStorage.getItem('zelana_witness') || '');

    if (!witnessToUse) {
      console.log('Verify: ERROR - no witness found! Setup may not have completed properly.');
      onLog(' Verification failed: Witness not found. Please re-run Setup.', 'error', 'verify');
      return;
    }

    console.log('Verify: final witnessToUse =', witnessToUse);
    console.log('Verify: salt check =', !!salt);
    console.log('Verify: witnessToUse check =', !!witnessToUse);

    if (!salt || !witnessToUse) {
      onLog(` Missing verification data - Salt: ${!!salt}, Witness: ${!!witnessToUse}. Please ensure you filled in the circuit input fields during Setup.`, 'error', 'verify');
      return;
    }

    setLoading(2);
    onLog(' Verifying proof...', 'info', 'verify');

    try {
      // For blind proofs, public_key is not required
      // if (!proofData.public_key) {
      //   onLog(' Proof is missing public_key. Please re-run Setup and Prove.', 'error', 'verify');
      //   setLoading(null);
      //   return;
      // }

      const blindProof = proofData.blind_proof || proofData; // Use the full blind proof if available

      // Use override salt if provided, otherwise use setup salt
      const saltHex = verifyOverrides.salt || (salt ? Array.from(salt).map(b => b.toString(16).padStart(2, '0')).join('') : '');

      if (!saltHex) {
        onLog(' Verification failed: Salt not available.', 'error', 'verify');
        return;
      }

      // Encode witness to hex for API
      const encoder = new TextEncoder();
      const witnessBytes = encoder.encode(witnessToUse);
      const witnessHex = Array.from(witnessBytes).map(b => b.toString(16).padStart(2, '0')).join('');

      const requestPayload = {
        blind_proof: blindProof,
        public_witness: witnessHex,
        salt: saltHex
      };

      const res = await fetch('/api/verify', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestPayload)
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(`HTTP ${res.status}: ${text}`);
      }

      const data: ApiResponse<{ valid: boolean; commitment_valid?: boolean }> = await res.json();

      setRequestResponses(prev => ({
        ...prev,
        2: {
          request: { blind_proof: '...', public_witness: witnessHex.substring(0, 20) + '...', salt: saltHex.substring(0, 16) + '...' },
          response: data,
          timestamp: new Date()
        }
      }));

      if (data.status === 'success') {
        setVerifyResult(data.data.valid);
        setCompletedSteps(prev => [...prev, 2]); // Mark verification step (2) as completed
        onLog(data.data.valid ? ' Proof is VALID!' : ' Proof is INVALID', data.data.valid ? 'success' : 'error', 'verify');
      } else {
        setVerifyResult(false); // Mark as failed
        setCompletedSteps(prev => [...prev, 2]); // Still mark as completed (attempted)
        onLog(` Verification failed: ${data.message}`, 'error', 'verify');
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Unknown error';
      setVerifyResult(false); // Mark as failed
      setCompletedSteps(prev => [...prev, 2]); // Still mark as completed (attempted)
      onLog(` Verification error: ${message}`, 'error', 'verify');
    } finally {
      setLoading(null);
    }
  };

  const handleStepAction = (stepId: number) => {
    if (stepId === 0) handleSetup();  // Setup blind proving session
    else if (stepId === 1) handleGetProof();  // Get proof after setup
    else if (stepId === 2) handleVerify();  // Verify proof
  };

  const renderInputField = (field: CircuitInputField) => {
    const value = circuitInputs[field.id] || '';
    const displayValue = value || field.defaultValue || '';
    const hasDefault = !value && field.defaultValue;
    const borderColor = field.isPrivate ? 'border-accent-red/40' : 'border-accent-green/40';
    const focusRing = field.isPrivate ? 'focus:ring-accent-red/50' : 'focus:ring-accent-green/50';

    return (
      <div key={field.id} className="mb-2">
        <label className="block text-[10px] font-medium text-text-secondary mb-1">
          {field.isPrivate ? '' : ''} {field.label}
          {hasDefault && <span className="text-accent-blue ml-1">(default: {field.defaultValue})</span>}
        </label>
        <input
          type={field.type === 'password' ? 'password' : 'text'}
          value={displayValue}
          onChange={(e) => handleInputChange(field.id, e.target.value)}
          placeholder={field.placeholder}
          className={`w-full px-2 py-1.5 bg-bg-primary border ${borderColor} rounded text-[11px] ${field.type === 'hex' ? 'font-mono' : ''
            } text-text-primary placeholder-text-tertiary focus:outline-none focus:ring-1 ${focusRing} ${hasDefault ? 'border-accent-blue/50' : ''}`}
          disabled={loading !== null}
        />
        {field.helpText && (
          <div className="text-[9px] text-text-tertiary mt-1">{field.helpText}</div>
        )}
      </div>
    );
  };

  const getCurrentRequest = (stepId: number) => {
    try {
      switch (stepId) {
        case 0: // Setup
          return setupPreview || {
            circuit_type: selectedCircuit,
            witness_commitment: { hash: 'computing...' },
            secret: 'computing...'
          };

        case 1: // Get Proof
          return {
            session_id: sessionId || 'will be generated by setup',
            circuit_type: selectedCircuit
          };

        case 2: // Verify
          const witnessToUse = publicWitness || (typeof window !== 'undefined' ? sessionStorage.getItem('zelana_witness') : null) || 'not available - run setup first';
          const saltHex = salt ? Array.from(salt).map(b => b.toString(16).padStart(2, '0')).join('') : 'not available - run setup first';

          return {
            blind_proof: proofData ? proofData : 'run get-proof first',
            public_witness: witnessToUse === 'not available - run setup first' ? witnessToUse : witnessToUse,
            salt: saltHex === 'not available - run setup first' ? saltHex : saltHex
          };

        default:
          return {};
      }
    } catch (error) {
      return { error: 'Error computing request preview' };
    }
  };

  const renderRequestResponse = (stepId: number) => {
    const data = requestResponses[stepId];
    const currentRequest = getCurrentRequest(stepId);

    return (
      <div className="text-[10px] mt-3">
        <div className="text-text-tertiary mb-2 font-semibold"> API Details</div>
        <div className="grid grid-cols-2 gap-2">
          <div className="bg-bg-tertiary rounded p-2">
            <div className="text-accent-blue font-semibold mb-1">Request</div>
            <pre className="text-[9px] font-mono overflow-x-auto max-h-35 text-text-secondary">
              {JSON.stringify(currentRequest, null, 1)}
            </pre>
          </div>
          <div className="bg-bg-tertiary rounded p-2">
            <div className="text-accent-green font-semibold mb-1">Response</div>
            {data ? (
              <pre className="text-[9px] font-mono overflow-x-auto max-h-35 text-text-secondary">
                {JSON.stringify(data.response, null, 1)}
              </pre>
            ) : (
              <div className="text-text-tertiary italic text-[9px]">Run to see response</div>
            )}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="p-4 h-full overflow-y-auto">
      <div className="max-w-6xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-base font-bold text-text-primary">Proof Workflow</h2>
            <p className="text-[10px] text-text-tertiary">Distributed ZK proof generation</p>
          </div>
          <div className="flex items-center gap-2">
            <div className="h-1.5 w-24 bg-bg-tertiary rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-accent-blue to-accent-green transition-all"
                style={{ width: `${(completedSteps.length / steps.length) * 100}%` }}
              />
            </div>
            <span className="text-[10px] text-text-tertiary">{completedSteps.length}/{steps.length}</span>
          </div>
        </div>

        {/* Circuit Selector */}
        <div className="flex gap-1 mb-4">
          {circuits.filter(c => c.status === 'active' || c.status === 'ui-only').map((circuit) => (
            <button
              key={circuit.id}
              onClick={() => handleCircuitChange(circuit.id)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs transition-all ${selectedCircuit === circuit.id
                ? 'bg-accent-blue text-white'
                : 'bg-bg-tertiary text-text-secondary hover:bg-bg-secondary'
                }`}
            >
              <span>{circuit.icon}</span>
              <span>{circuit.name}</span>
            </button>
          ))}
        </div>

        {/* Circuit Info */}
        {currentMetadata && (
          <div className="mb-4 bg-bg-secondary rounded-lg p-3 border border-border">
            <div className="flex items-center gap-2 mb-2">
              <span className="text-xl">{currentMetadata.icon}</span>
              <div>
                <span className="text-sm font-semibold text-text-primary">{currentMetadata.name}</span>
                <span className="text-[10px] text-text-tertiary ml-2">{currentMetadata.useCase}</span>
              </div>
            </div>
            <div className="text-[10px] font-mono text-accent-purple bg-accent-purple/10 px-2 py-1 rounded inline-block">
              {currentMetadata.statement}
            </div>
            <div className="flex gap-4 mt-2 text-[10px]">
              <div>
                <span className="text-accent-green"> Public:</span>
                <span className="text-text-secondary ml-1">{currentMetadata.publicInputs.join(', ')}</span>
              </div>
              <div>
                <span className="text-accent-red"> Private:</span>
                <span className="text-text-secondary ml-1">{currentMetadata.privateWitness.join(', ')}</span>
              </div>
            </div>
          </div>
        )}

        {/* Main Workflow - Steps */}
        <div className="grid grid-cols-1 gap-3 mb-4">
          {steps.map((step) => {
            const isCompleted = completedSteps.includes(step.id);
            const isVerifySuccess = step.id === 2 && verifyResult === true;
            const isStepSuccessful = isCompleted && (step.id !== 2 || isVerifySuccess);
            const isCurrent = currentStep === step.id;
            const isAvailable = step.id === 0 || completedSteps.includes(step.id - 1);

            return (
              <div
                key={step.id}
                className={`rounded-lg p-3 border transition-all ${isStepSuccessful
                  ? 'bg-accent-green/5 border-accent-green/30'
                  : isCurrent
                    ? 'bg-accent-blue/5 border-accent-blue/30'
                    : 'bg-bg-secondary border-border'
                  }`}
              >
                <div className="flex flex-col sm:flex-row gap-4">
                  {/* Left side - Main content */}
                  <div className="flex-1 min-w-0">
                    {/* Step Header */}
                    <div className="flex items-center gap-2 mb-2">
                      <div
                        className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold ${isStepSuccessful
                          ? 'bg-accent-green text-white'
                          : isCurrent
                            ? 'bg-accent-blue text-white'
                            : 'bg-bg-tertiary text-text-tertiary'
                          }`}
                      >
                        {isStepSuccessful ? '' : step.id + 1}
                      </div>
                      <span className="text-sm font-semibold text-text-primary">{step.title}</span>
                      <span className="text-lg">{step.icon}</span>
                    </div>

                    {/* Step Content */}
                    <div className="min-h-[120px]">
                      {step.id === 0 && currentHandler && (
                        <div className="space-y-2">
                          {currentHandler.setupFields.map(field => renderInputField(field))}
                          {previewSalt && previewSalt !== 'circuit not available' && previewSalt !== 'error computing preview' && setupPreview?.witness_commitment?.witness && (
                            <div className="mb-3 p-2 bg-accent-blue/5 border border-accent-blue/20 rounded text-[10px]">
                              <div className="text-accent-blue font-semibold mb-2"> Hash Generator</div>
                              <div className="space-y-1">
                                <div className="flex items-center gap-2">
                                  <span className="text-accent-green font-semibold">Witness:</span>
                                  <span className="font-mono text-accent-green break-all">"{setupPreview.witness_commitment.witness}"</span>
                                </div>
                                <div className="flex items-center gap-2">
                                  <span className="text-accent-red font-semibold">Salt:</span>
                                  <span className="font-mono text-accent-red break-all">{previewSalt}</span>
                                </div>
                                <div className="flex items-center gap-2">
                                  <span className="text-accent-blue font-semibold">SHA-256:</span>
                                  <span className="font-mono text-accent-blue break-all">{previewHash}</span>
                                </div>
                              </div>
                            </div>
                          )}


                        </div>
                      )}

                      {step.id === 1 && (
                        <div className="text-[10px] text-text-secondary space-y-1">
                          {setupData ? (
                            <>
                              <div className="flex justify-between">
                                <span>Threshold:</span>
                                <span className="font-mono text-text-primary">{setupData.threshold}/{setupData.num_nodes}</span>
                              </div>
                              <div className="flex justify-between">
                                <span>Session:</span>
                                <span className="font-mono text-text-primary truncate max-w-[100px]">{sessionId.substring(0, 16)}...</span>
                              </div>
                              {currentHandler?.proveFields?.map(field => {
                                const value = circuitExtraData[field.id] as string || '';
                                return value && (
                                  <div key={field.id} className="flex justify-between">
                                    <span>{field.label.split(':')[0]}:</span>
                                    <span className="font-mono text-text-primary truncate max-w-[100px]">{value.substring(0, 16)}...</span>
                                  </div>
                                );
                              })}
                            </>
                          ) : (
                            <div className="text-text-tertiary">Complete setup first</div>
                          )}
                        </div>
                      )}

                      {step.id === 2 && (
                        <div className="space-y-2">
                          {/* Proof Data Display */}
                          <div className="text-[10px] text-text-secondary space-y-1">
                            {proofData ? (
                              <>
                                <div className="flex justify-between">
                                  <span>Commitment:</span>
                                  <span className="font-mono text-text-primary truncate max-w-[100px]">{proofData.commitment.substring(1, 17)}...</span>
                                </div>
                                <div className="flex justify-between">
                                  <span>Challenge:</span>
                                  <span className="font-mono text-text-primary truncate max-w-[100px]">{proofData.challenge.substring(0, 16)}...</span>
                                </div>
                                <div className="flex justify-between">
                                  <span>Response:</span>
                                  <span className="font-mono text-text-primary truncate max-w-[100px]">{proofData.response.substring(0, 16)}...</span>
                                </div>
                              </>
                            ) : (
                              <div className="text-text-tertiary">Generate proof first</div>
                            )}
                          </div>

                          {/* Verify Override Inputs */}
                          {proofData && (
                            <div className="space-y-2 pt-2 border-t border-border">
                              <div className="text-[10px] font-semibold text-accent-orange mb-1">
                                 Test Parameters (modify to test false verification)
                              </div>
                              <div className="space-y-1">
                                <label className="block text-[9px] font-medium text-text-secondary">
                                  Witness Override
                                </label>
                                <input
                                  type="text"
                                  value={(verifyOverrides.witness ?? publicWitness) || ''}
                                  onChange={(e) => setVerifyOverrides(prev => ({ ...prev, witness: e.target.value }))}
                                  placeholder="Leave empty to use setup witness"
                                  className="w-full px-1 py-1 bg-bg-primary border border-accent-orange/40 rounded text-[9px] font-mono text-text-primary"
                                />
                              </div>
                              <div className="space-y-1">
                                <label className="block text-[9px] font-medium text-text-secondary">
                                  Salt Override (hex)
                                </label>
                                <input
                                  type="text"
                                  value={(verifyOverrides.salt ?? (salt ? Array.from(salt).map(b => b.toString(16).padStart(2, '0')).join('') : ''))}
                                  onChange={(e) => setVerifyOverrides(prev => ({ ...prev, salt: e.target.value }))}
                                  placeholder="Leave empty to use setup salt"
                                  className="w-full px-1 py-1 bg-bg-primary border border-accent-orange/40 rounded text-[9px] font-mono text-text-primary"
                                />
                              </div>
                            </div>
                          )}
                        </div>
                      )}
                    </div>

                    {/* Action Button */}
                    <button
                      onClick={() => handleStepAction(step.id)}
                      disabled={!isAvailable || loading !== null || !isCircuitActive(selectedCircuit)}
                      className={`w-full mt-3 px-3 py-1.5 rounded text-xs font-medium transition-all ${!isAvailable || loading !== null || !isCircuitActive(selectedCircuit)
                        ? 'bg-bg-tertiary text-text-tertiary cursor-not-allowed'
                        : (isCompleted && (step.id !== 2 || verifyResult === true))
                          ? 'bg-accent-green hover:bg-accent-green/80 text-white'
                          : 'bg-accent-blue hover:bg-accent-blue/80 text-white'
                        }`}
                    >
                      {loading === step.id ? '⏳ Processing...' : (isCompleted && (step.id !== 2 || verifyResult === true)) ? '↻ Re-run' : `▶ Run`}
                    </button>

                    {/* Result Badge */}
                    {step.id === 2 && verifyResult !== null && (
                      <div
                        className={`mt-2 py-1 px-2 rounded text-center text-xs font-semibold ${verifyResult ? 'bg-accent-green/20 text-accent-green' : 'bg-accent-red/20 text-accent-red'
                          }`}
                      >
                        {verifyResult ? ' Valid' : ' Invalid'}
                      </div>
                    )}
                  </div>

                  {/* Right side - API Details */}
                  <div className="w-full sm:w-200 shrink-0">
                    {renderRequestResponse(step.id)}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
        {/* Privacy Details */}
        <div className="mb-4">
          <details>
            <summary
              className="cursor-pointer text-sm font-semibold text-accent-purple flex items-center gap-2 mb-2"
            >
              ️ Blind Proving Details
            </summary>

            <div className="bg-gradient-to-br from-purple-900/10 to-blue-900/10 border border-purple-500/30 rounded-lg p-3">
              <div className="grid grid-cols-3 gap-2 mb-3">
                {shares.map((share) => (
                  <div key={share.node_id} className="bg-bg-primary/50 rounded p-2 text-center">
                    <div className="w-6 h-6 mx-auto bg-purple-600 rounded-full flex items-center justify-center text-white text-xs font-bold mb-1">
                      {share.node_id}
                    </div>
                    <div className="text-[9px] text-text-secondary">Share #{share.share_index}</div>
                    <div className="text-[8px] text-purple-300">️ Witness Hidden</div>
                  </div>
                ))}
              </div>

              <div className="text-[10px] text-text-secondary space-y-1">
                <div className="flex items-center gap-1">
                  <span className="text-purple-400"></span>
                  <span>Public witness <span className="font-semibold text-text-primary">&quot;{publicWitness}&quot;</span> never sent to nodes</span>
                </div>
                <div className="flex items-center gap-1">
                  <span className="text-purple-400"></span>
                  <span>Nodes only receive commitment hash + secret share</span>
                </div>
                <div className="flex items-center gap-1">
                  <span className="text-purple-400"></span>
                  <span>Witness revealed only at verification time</span>
                </div>
              </div>
            </div>
          </details>
        </div>

      </div>
    </div>
  );
}