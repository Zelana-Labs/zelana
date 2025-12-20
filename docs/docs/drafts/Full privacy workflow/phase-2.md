## Phase 2 — Encrypted mempool submission (privacy vs sequencer)
### 2.1 Encrypt TxObject to committee
Wallet encrypts TxObject:
- `cipher = Encrypt(PK_committee, TxObj)`
- produces `cipher_commitment = Hash(cipher)`
### 2.2 Send to relayer (recommended)
Wallet → Relayer:
- sends `cipher` (and maybe fee token for relayer)

Relayer → committee mempool:

- forwards `cipher` to multiple sequencers    

**Sequencers see only ciphertexts at this stage.**
