'use client';

import { useState } from 'react';

export default function HashGenerator() {
  const [input, setInput] = useState('my_secret_password');
  const [hash, setHash] = useState('');
  const [copying, setCopying] = useState(false);

  const generateHash = async () => {
    if (!input) {
      setHash('');
      return;
    }

    try {
      // Use Web Crypto API for SHA-256
      const encoder = new TextEncoder();
      const data = encoder.encode(input);
      const hashBuffer = await crypto.subtle.digest('SHA-256', data);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      const hashHex = '0x' + hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
      setHash(hashHex);
      } catch {
      setHash('Error generating hash');
    }
  };

  const copyToClipboard = async () => {
    if (hash) {
      await navigator.clipboard.writeText(hash);
      setCopying(true);
      setTimeout(() => setCopying(false), 1500);
    }
  };

  return (
    <div className="bg-bg-secondary rounded-lg p-4 border border-border">
      <h3 className="text-sm font-semibold text-text-primary mb-3 flex items-center gap-2">
        <span>🔗</span>
        <span>Hash Generator</span>
      </h3>
      <p className="text-xs text-text-tertiary mb-3">
        Compute SHA-256 hash for testing hash preimage circuits
      </p>

      <div className="space-y-3">
        {/* Input */}
        <div>
          <label className="block text-xs font-medium text-text-secondary mb-2">
            Input (Preimage)
          </label>
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Enter text to hash"
            className="w-full px-3 py-2 bg-bg-primary border border-border rounded-lg text-xs text-text-primary placeholder-text-tertiary focus:outline-none focus:ring-2 focus:ring-accent-blue transition-all"
          />
        </div>

        {/* Generate Button */}
        <button
          onClick={generateHash}
          className="w-full px-4 py-2 bg-accent-blue hover:bg-accent-blue/80 rounded-lg text-xs font-medium text-white transition-all duration-200"
        >
          ⚡ Generate Hash
        </button>

        {/* Output */}
        {hash && (
          <div>
            <label className="block text-xs font-medium text-text-secondary mb-2">
              SHA-256 Hash
            </label>
            <div className="relative">
              <div className="w-full px-3 py-2 bg-bg-primary border border-accent-green/50 rounded-lg text-xs font-mono text-accent-green break-all">
                {hash}
              </div>
              <button
                onClick={copyToClipboard}
                className="absolute top-2 right-2 px-2 py-1 bg-bg-tertiary hover:bg-bg-secondary rounded text-[10px] text-text-secondary transition-all"
                title="Copy to clipboard"
              >
                {copying ? '✓ Copied' : '📋'}
              </button>
            </div>
            <p className="text-[10px] text-text-tertiary mt-1">
              This is the public hash that everyone can see. The input above is the private preimage.
            </p>
          </div>
        )}

        {/* Info Box */}
        <div className="bg-accent-blue/10 border border-accent-blue/30 rounded-lg p-3">
          <div className="flex items-start gap-2">
            <span className="text-sm">💡</span>
            <div>
              <div className="text-xs font-semibold text-accent-blue mb-1">How to use</div>
              <ul className="text-[10px] text-text-secondary space-y-1">
                <li>• Enter your secret text as input</li>
                 <li>• Click &quot;Generate Hash&quot; to compute SHA-256</li>
                <li>• Copy the hash to use as public input</li>
                 <li>• Use the original text as private &quot;witness&quot;</li>
              </ul>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
