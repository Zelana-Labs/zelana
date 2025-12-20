## Failure modes (what happens if something goes wrong)
### Sequencer lies about data
- wallet detects hash mismatch vs Solana `batch_hash`
- fetch from another source
### Sequencer withholds data
- use another sequencer/cache/DA replica
- if DA is robust, withholding can’t trap users
### Malicious state transition attempt
- proof fails on Solana
- batch not accepted
- nothing reverts (it never finalized)
