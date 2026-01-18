/**
 * Zelana React SDK
 * 
 * React hooks and components for interacting with Zelana L2.
 * 
 * @example
 * ```tsx
 * import { ZelanaProvider, useBalance, useTransfer, Keypair } from '@zelana/react';
 * 
 * // Wrap your app with the provider
 * function App() {
 *   return (
 *     <ZelanaProvider baseUrl="http://localhost:3000">
 *       <Wallet />
 *     </ZelanaProvider>
 *   );
 * }
 * 
 * // Use hooks in your components
 * function Wallet() {
 *   const { connect, disconnect, isConnected } = useZelana();
 *   const { balance, isLoading } = useBalance();
 *   const { mutate: transfer, isLoading: isSending } = useTransfer();
 * 
 *   const handleConnect = () => {
 *     const keypair = Keypair.generate();
 *     connect(keypair);
 *   };
 * 
 *   const handleSend = async () => {
 *     await transfer({
 *       to: recipientAddress,
 *       amount: 1_000_000n,
 *     });
 *   };
 * 
 *   return (
 *     <div>
 *       {isConnected ? (
 *         <>
 *           <p>Balance: {balance?.toString()}</p>
 *           <button onClick={handleSend} disabled={isSending}>
 *             Send
 *           </button>
 *           <button onClick={disconnect}>Disconnect</button>
 *         </>
 *       ) : (
 *         <button onClick={handleConnect}>Connect</button>
 *       )}
 *     </div>
 *   );
 * }
 * ```
 * 
 * @packageDocumentation
 */

// Context & Provider
export { ZelanaProvider, useZelanaContext } from './context';
export type { ZelanaContextValue, ZelanaProviderProps } from './context';

// Hooks
export {
  // Core
  useZelana,
  useHealth,
  useAccount,
  useBalance,
  useStateRoots,
  useBatchStatus,
  useStats,
  
  // Transactions
  useTransfer,
  useWithdraw,
  useTransaction,
  useWaitForTransaction,
  
  // Lists
  useBatches,
  useTransactions,
} from './hooks';

export type {
  UseQueryResult,
  UseMutationResult,
} from './hooks';

// Re-export commonly used types from SDK
export { Keypair, PublicKey, ZelanaError } from '@zelana/sdk';
export type {
  AccountState,
  TransferResponse,
  WithdrawResponse,
  TxSummary,
  BatchSummary,
  HealthInfo,
  StateRoots,
  GlobalStats,
} from '@zelana/sdk';
