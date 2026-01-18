use sha2::{Digest, Sha256};
use std::collections::HashMap;
use zelana_account::{AccountId, AccountState};
use zelana_block::{BlockHeader, HEADER_MAGIC, HEADER_VERSION};

use crate::sequencer::execution::executor::ExecutionResult;

#[derive(Clone)]
pub struct Session {
    pub batch_id: u64,
    pub txs: Vec<ExecutionResult>,
}

impl Session {
    pub fn new(batch_id: u64) -> Self {
        Self {
            batch_id,
            txs: Vec::new(),
        }
    }
    pub fn push_execution(&mut self, exec: ExecutionResult) {
        self.txs.push(exec);
    }

    pub fn tx_count(&self) -> u32 {
        self.txs.len() as u32
    }

    pub fn close(self, prev_root: [u8; 32], new_root: [u8; 32]) -> ClosedSession {
        let header = BlockHeader {
            magic: HEADER_MAGIC,
            hdr_version: HEADER_VERSION,
            batch_id: self.batch_id,
            prev_root,
            new_root,
            tx_count: self.tx_count(),
            open_at: chrono::Utc::now().timestamp() as u64,
            flags: 0,
        };

        ClosedSession {
            header,
            txs: self.txs,
        }
    }
}

// ready to commit ( closed session)
pub struct ClosedSession {
    pub header: BlockHeader,
    pub txs: Vec<ExecutionResult>,
}

pub fn compute_state_root(base_state: &HashMap<AccountId, AccountState>) -> [u8; 32] {
    let mut items: Vec<_> = base_state.iter().collect();
    items.sort_by_key(|(id, _)| id.to_hex());

    let mut hasher = Sha256::new();
    for (id, st) in items {
        hasher.update(id.as_ref());
        hasher.update(&st.balance.to_be_bytes());
        hasher.update(&st.nonce.to_be_bytes());
    }

    hasher.finalize().into()
}
