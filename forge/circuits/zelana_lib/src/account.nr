// Account State Management
//
// Each account has:
// - pubkey: The Ed25519 public key (also serves as account ID)
// - balance: The current balance in lamports
// - nonce: Transaction counter for replay protection
//
// The account leaf in the Merkle tree is:
//   leaf = Poseidon(pubkey, balance, nonce)

use crate::poseidon::{hash_4, domain_account};

/// Account structure
pub struct Account {
    /// Account public key / ID
    pub pubkey: Field,
    /// Current balance
    pub balance: Field,
    /// Transaction nonce
    pub nonce: Field,
}

impl Account {
    /// Create a new account
    pub fn new(pubkey: Field, balance: Field, nonce: Field) -> Self {
        Account { pubkey, balance, nonce }
    }
    
    /// Create an empty account (for new recipients)
    pub fn empty(pubkey: Field) -> Self {
        Account { pubkey, balance: 0, nonce: 0 }
    }
    
    /// Compute the leaf hash for this account
    pub fn to_leaf(self) -> Field {
        compute_account_leaf(self.pubkey, self.balance, self.nonce)
    }
    
    /// Apply a transfer (debit)
    pub fn debit(self, amount: Field) -> Self {
        // Note: Caller must verify sufficient balance
        Account {
            pubkey: self.pubkey,
            balance: self.balance - amount,
            nonce: self.nonce + 1,
        }
    }
    
    /// Apply a transfer (credit)
    pub fn credit(self, amount: Field) -> Self {
        Account {
            pubkey: self.pubkey,
            balance: self.balance + amount,
            nonce: self.nonce,  // Receiver nonce doesn't change
        }
    }
    
    /// Check if account has sufficient balance
    pub fn has_balance(self, amount: Field) -> bool {
        // Convert to u64 for comparison
        let balance_u64 = self.balance as u64;
        let amount_u64 = amount as u64;
        balance_u64 >= amount_u64
    }
}

/// Compute the Merkle leaf for an account
pub fn compute_account_leaf(pubkey: Field, balance: Field, nonce: Field) -> Field {
    // Domain-separated hash: H(domain, pubkey, balance, nonce)
    hash_4(domain_account(), pubkey, balance, nonce)
}

/// Transfer state transition
/// 
/// Computes new sender and receiver accounts after a transfer.
/// Returns (new_sender, new_receiver)
pub fn apply_transfer(
    sender: Account,
    receiver: Account,
    amount: Field
) -> (Account, Account) {
    // Verify sender has sufficient balance
    assert(sender.has_balance(amount), "Insufficient balance");
    
    let new_sender = sender.debit(amount);
    let new_receiver = receiver.credit(amount);
    
    (new_sender, new_receiver)
}

#[test]
fn test_account_leaf() {
    let acc = Account::new(123, 1000, 5);
    let leaf = acc.to_leaf();
    
    // Verify leaf is deterministic
    let leaf2 = compute_account_leaf(123, 1000, 5);
    assert(leaf == leaf2);
}

#[test]
fn test_transfer() {
    let sender = Account::new(1, 1000, 0);
    let receiver = Account::new(2, 500, 0);
    
    let (new_sender, new_receiver) = apply_transfer(sender, receiver, 300);
    
    assert(new_sender.balance == 700);
    assert(new_sender.nonce == 1);
    assert(new_receiver.balance == 800);
    assert(new_receiver.nonce == 0);
}
