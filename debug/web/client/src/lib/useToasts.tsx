/**
 * Toast Notification System
 *
 * Displays real-time notifications for transaction and settlement events
 * with clickable links to Solana Explorer.
 */

import { useState, useCallback, useEffect } from "react";
import { createPortal } from "react-dom";
import { X, ExternalLink, CheckCircle, AlertCircle, Info, AlertTriangle } from "lucide-react";
import type { ToastEvent } from "./useWebSocket";

interface ToastProps {
  toast: ToastEvent;
  onDismiss: (id: string) => void;
}

function Toast({ toast, onDismiss }: ToastProps) {
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    // Auto-dismiss after 8 seconds
    const timer = setTimeout(() => {
      setIsExiting(true);
      setTimeout(() => onDismiss(toast.id), 300);
    }, 8000);

    return () => clearTimeout(timer);
  }, [toast.id, onDismiss]);

  const handleDismiss = () => {
    setIsExiting(true);
    setTimeout(() => onDismiss(toast.id), 300);
  };

  const icons = {
    success: <CheckCircle size={18} className="text-accent-green" />,
    error: <AlertCircle size={18} className="text-accent-red" />,
    warning: <AlertTriangle size={18} className="text-accent-yellow" />,
    info: <Info size={18} className="text-accent-cyan" />,
  };

  const borderColors = {
    success: "border-l-accent-green",
    error: "border-l-accent-red",
    warning: "border-l-accent-yellow",
    info: "border-l-accent-cyan",
  };

  const truncateHash = (hash: string, chars = 8) => {
    if (hash.length <= chars * 2) return hash;
    return `${hash.slice(0, chars)}...${hash.slice(-chars)}`;
  };

  return (
    <div
      className={`
        bg-bg-secondary border border-border border-l-4 ${borderColors[toast.type]}
        rounded-lg shadow-lg p-4 max-w-sm w-full
        transform transition-all duration-300 ease-out
        ${isExiting ? "opacity-0 translate-x-full" : "opacity-100 translate-x-0"}
      `}
    >
      <div className="flex items-start gap-3">
        <div className="flex-shrink-0 mt-0.5">{icons[toast.type]}</div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between gap-2">
            <h4 className="font-medium text-text-primary text-sm">{toast.title}</h4>
            <button
              onClick={handleDismiss}
              className="flex-shrink-0 p-1 text-text-muted hover:text-text-primary transition-colors"
            >
              <X size={14} />
            </button>
          </div>
          <p className="text-text-secondary text-xs mt-1">{toast.message}</p>

          {/* Transaction hash link */}
          {toast.txHash && (
            <div className="mt-2">
              <span className="text-text-muted text-xs">TX: </span>
              <span className="font-mono text-xs text-accent-purple">
                {truncateHash(toast.txHash)}
              </span>
            </div>
          )}

          {/* L1 settlement link */}
          {toast.l1TxSig && (
            <a
              href={`https://explorer.solana.com/tx/${toast.l1TxSig}?cluster=devnet`}
              target="_blank"
              rel="noopener noreferrer"
              className="mt-2 inline-flex items-center gap-1 text-xs text-accent-cyan hover:underline"
            >
              View on Solana Explorer
              <ExternalLink size={10} />
            </a>
          )}
        </div>
      </div>
    </div>
  );
}

interface ToastContainerProps {
  toasts: ToastEvent[];
  onDismiss: (id: string) => void;
}

function ToastContainer({ toasts, onDismiss }: ToastContainerProps) {
  return createPortal(
    <div className="fixed top-4 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <Toast key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>,
    document.body
  );
}

export function useToasts() {
  const [toasts, setToasts] = useState<ToastEvent[]>([]);

  const addToast = useCallback((toast: ToastEvent) => {
    setToasts((prev) => {
      // Prevent duplicate toasts
      if (prev.some((t) => t.id === toast.id)) {
        return prev;
      }
      // Keep max 5 toasts
      const newToasts = [toast, ...prev].slice(0, 5);
      return newToasts;
    });
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const ToastRenderer = useCallback(
    () => <ToastContainer toasts={toasts} onDismiss={dismissToast} />,
    [toasts, dismissToast]
  );

  return {
    addToast,
    dismissToast,
    ToastRenderer,
  };
}
