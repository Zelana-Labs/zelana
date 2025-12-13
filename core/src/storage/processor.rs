use crate::storage::{StateStore};
use zelana_transaction::{TransactionType, SignedTransaction, DepositEvent, WithdrawRequest};
use anyhow::{Result,bail};

pub struct BatchExecutor<'a,S:StateStore>{
    store: &'a mut S
}

impl<'a, S:StateStore> BatchExecutor<'a,S>{
    pub fn new(store: &'a mut S) -> Self {
        Self { store }
    }
    /// Entry point for applying a generic L2 Transaction.
    pub fn execute(&mut self, tx: &TransactionType) -> Result<()> {
        match tx {
            TransactionType::Transfer(signed_tx) => self.execute_transfer(signed_tx),
            TransactionType::Deposit(deposit) => self.execute_deposit(deposit),
            TransactionType::Withdraw(req) => self.execute_withdraw(req),
        }
    }
    fn execute_transfer(&mut self, tx: &SignedTransaction) -> Result<()> {
        //Verify Signature
        // In the ZKVM, we assume signature checked by the main loop witness verification.
        
        let from_id = tx.data.from;
        let to_id = tx.data.to;
        let amount = tx.data.amount;
        let nonce = tx.data.nonce;

        //Load Sender
        let mut sender = self.store.get_account(&from_id)?;

        //Checks
        if sender.nonce != nonce {
            bail!("Nonce mismatch: expected {}, got {}", sender.nonce, nonce);
        }
        if sender.balance < amount {
            bail!("Insufficient funds: balance {}, needed {}", sender.balance, amount);
        }

        //Update Sender
        sender.balance -= amount;
        sender.nonce += 1;
        self.store.set_account(from_id, sender)?;

        //Update Recipient
        let mut recipient = self.store.get_account(&to_id)?;
        recipient.balance = recipient.balance.checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("Overflow in recipient balance"))?;
        self.store.set_account(to_id, recipient)?;

        Ok(())
    }

    fn execute_deposit(&mut self, deposit: &DepositEvent) -> Result<()> {
        // Deposits are authoritative "Mint" events from L1.
        // We do not check nonces or signatures (L1 Bridge did that).
        let mut account = self.store.get_account(&deposit.to)?;
        
        account.balance = account.balance.checked_add(deposit.amount)
            .ok_or_else(|| anyhow::anyhow!("Overflow in deposit"))?;
            
        self.store.set_account(deposit.to, account)?;
        Ok(())
    }

    fn execute_withdraw(&mut self, req: &WithdrawRequest) -> Result<()> {
        let mut sender = self.store.get_account(&req.from)?;

        if sender.nonce != req.nonce {
            bail!("Nonce mismatch on withdraw");
        }
        if sender.balance < req.amount {
            bail!("Insufficient funds for withdraw");
        }

        // Burn funds on L2
        sender.balance -= req.amount;
        sender.nonce += 1;
        
        self.store.set_account(req.from, sender)?;
        Ok(())
    }
}