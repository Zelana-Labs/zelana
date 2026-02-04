// Circuit configuration types for the pluggable circuit system

/**
 * Circuit status - determines availability in the UI
 */
export type CircuitStatus = 'active' | 'coming-soon' | 'ui-only';

/**
 * Circuit metadata - describes the circuit in the UI
 */
export interface CircuitMetadata {
  id: string;           // Unique identifier (kebab-case, matches backend)
  name: string;         // Display name
  icon: string;         // Emoji icon
  description: string;  // Short description
  statement: string;    // Mathematical statement being proved
  publicInputs: string[];   // List of public inputs
  privateWitness: string[]; // List of private witnesses
  useCase: string;      // Example use cases
  status: CircuitStatus;
}

/**
 * Input field definition for dynamic form generation
 */
export interface CircuitInputField {
  id: string;
  label: string;
  placeholder: string;
  type: 'text' | 'password' | 'hex' | 'number';
  isPrivate: boolean;   // Red border if private, green if public
  defaultValue?: string;
  helpText?: string;
  validation?: {
    minLength?: number;
    maxLength?: number;
    pattern?: RegExp;
  };
}

/**
 * Circuit setup inputs - what the user provides
 */
export interface CircuitSetupInputs {
  [key: string]: string;
}

/**
 * Circuit setup result - what gets sent to the backend
 */
export interface CircuitSetupPayload {
  secret: string;       // The secret to be split via Shamir
  witness: string;      // The public witness
  extraData?: Record<string, unknown>; // Additional circuit-specific data
}

/**
 * Circuit handler - defines how each circuit processes inputs
 */
export interface CircuitHandler {
  metadata: CircuitMetadata;

  // Input fields for Setup step
  setupFields: CircuitInputField[];

  // Input fields for Prove step (optional, shown after setup)
  proveFields?: CircuitInputField[];

  // Process user inputs into setup payload
  processSetup: (inputs: CircuitSetupInputs) => Promise<CircuitSetupPayload>;

  // Custom verification logs (optional)
  getVerificationLogs?: (isValid: boolean) => string[];

  // Info banner content (optional)
  infoBanner?: {
    title: string;
    description: string;
    howItWorks: string[];
    tip?: string;
  };
}

/**
 * Circuit registry type
 */
export type CircuitRegistry = Record<string, CircuitHandler>;
