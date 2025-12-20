## Minimal “MVP” simplifications 
- Start with **no encrypted mempool**, just relayers + opaque TxObject (still private contents)
- Start with **single sequencer**
- Keep DA in one replicated store + anchor hash on Solana  
    Then iterate toward:
	- committee sequencing
	- threshold encryption
	- recursive proving