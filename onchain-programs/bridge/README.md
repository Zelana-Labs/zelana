# Zelana Bridge

The Zelana Bridge is a Solana-based program built using the [Pinocchio](https://github.com/anza-xyz/pinocchio). It enables the transfer of assets with a sequencer-based withdrawal mechanism, suggesting it is designed for integrations that require attested or privacy-preserving transactions.

## Overview

The bridge operates on a simple yet powerful model:

*   **Initialization**: A one-time setup to configure the bridge's operational parameters, including a trusted sequencer.
*   **Deposits**: Users can deposit assets into a secure vault, and a receipt is generated on-chain to prove the transaction.
*   **Withdrawals**: Withdrawals are processed by a trusted sequencer, which validates the request and authorizes the release of funds. This process uses a nullifier to prevent replay attacks and ensure that each withdrawal is unique.

## How It Works

The bridge's functionality is divided into three main instructions:

1.  **`Initialize`**: This instruction sets up the bridge by creating a `Config` account and a `Vault` account. The `Config` account stores essential parameters, such as the `sequencer_authority`, which is the public key of the trusted sequencer authorized to approve withdrawals. The `Vault` is a PDA that securely holds all deposited assets.

2.  **`Deposit`**: When a user deposits assets, the `process_deposit` function transfers the funds into the `Vault`. It then creates a `DepositReceipt` account, which serves as an on-chain record of the deposit, containing details like the depositor's public key, the amount, a unique nonce, and a timestamp.

3.  **`WithdrawAttested`**: The withdrawal process is managed by the `process_withdraw_attested` function and initiated by the `sequencer`. To withdraw funds, the sequencer provides a unique `nullifier` to prevent double-spending. The bridge verifies the sequencer's authority and then transfers the specified amount from the `Vault` to the recipient's account. A `UsedNullifier` account is created to ensure the same nullifier cannot be used again.

## Accounts

The bridge uses the following on-chain accounts:

*   **`Config`**: Stores the bridge's configuration, including the `sequencer_authority` and a `domain` identifier.
*   **`Vault`**: A PDA that holds all deposited assets.
*   **`DepositReceipt`**: A PDA created for each deposit, acting as a proof of the transaction.
*   **`UsedNullifier`**: A PDA created for each withdrawal to mark a nullifier as used and prevent replay attacks.

## Instructions

The program exposes the following instructions:

| Instruction          | Description                                                                                             | Accounts Required                                                                |
| -------------------- | ------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `Initialize`         | Initializes the bridge by creating the `Config` and `Vault` accounts.                                   | `payer`, `config_account`, `vault_account`, `system_program`                     |
| `Deposit`            | Deposits assets into the bridge and creates a `DepositReceipt`.                                         | `depositor`, `config_account`, `vault_account`, `deposit_receipt_account`, `system_program` |
| `WithdrawAttested`   | Withdraws assets from the bridge, authorized by the sequencer and using a unique nullifier.             | `sequencer`, `config_account`, `vault_account`, `recipient`, `user_nullifier_account`, `system_program` |

## Building and Testing

To build and test the program, you can use the following commands:

### Build
```bash
cargo build-sbf
```

### Test
```bash
cargo test
```

## Program ID
The on-chain program ID for the Zelana Bridge is: `95sWqtU9fdm19cvQYu94iKijRuYAv3wLqod1pcsSfYth`