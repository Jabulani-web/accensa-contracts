# Roadmap

## Issue #443: stACC liquid staking derivative minting

- [x] Implement stACC liquid staking derivative minting (#443)
  - New `liquid_staking` module in `contracts/governance/src/liquid_staking.rs`
  - 1:1 mint/burn of stACC against locked underlying tokens
  - Exchange rate progression and lock-epoch-based redemption
  - CHANGELOG.md entry added

## Issue #693: typed missing-state error in authorize_with_slippage

- [x] `authorize_with_slippage` reports a missing authorization record as `Error::AuthorizationNotFound` instead of unwrapping, and a pre-init call yields `Error::NotInitialized` (#693)