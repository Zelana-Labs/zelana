/// Event coming from the L1 Listener.
#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub struct DepositEvent {
    pub to: AccountId,
    pub amount: u64,
    pub l1_seq: u64,
}

#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub struct WithdrawRequest {
    pub from: AccountId,
    pub to_l1_address: [u8; 32],
    pub amount: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
    pub signer_pubkey: [u8; 32],
}