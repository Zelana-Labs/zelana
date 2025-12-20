## Phase 0 — Setup (once)
1. **Key systems**
	- Wallet has:
	    - spending key
	    - viewing key (for decrypting outputs)
	- Sequencer committee has:
	    - threshold encryption key shares (for mempool encryption)
	    - signing keys (for batch approvals)

1. **Rollup state on Solana**
	- program stores:
	    - `state_root`
	    - `nullifier_root`
	    - `batch_index`
