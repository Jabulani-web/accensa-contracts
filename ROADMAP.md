# Roadmap

## Issue #443: stACC liquid staking derivative minting

- [x] Implement stACC liquid staking derivative minting (#443)
  - New `liquid_staking` module in `contracts/governance/src/liquid_staking.rs`
  - 1:1 mint/burn of stACC against locked underlying tokens
  - Exchange rate progression and lock-epoch-based redemption
  - CHANGELOG.md entry added

## Issue #645: timelock approvals require a registered signer

- [x] `approve_queued_transaction` rejects unregistered addresses with `Error::UnknownSigner` before writing, with stranger- and signer-approval coverage (#645)