# Roadmap

## Issue #443: stACC liquid staking derivative minting

- [x] Implement stACC liquid staking derivative minting (#443)
  - New `liquid_staking` module in `contracts/governance/src/liquid_staking.rs`
  - 1:1 mint/burn of stACC against locked underlying tokens
  - Exchange rate progression and lock-epoch-based redemption
  - CHANGELOG.md entry added

## Issue #647: unit tests for the vesting curve model

- [x] `contracts/treasury/src/vesting.rs` gains an inline `#[cfg(test)]` module pinning the cliff boundaries, mid-window floor rounding, `claimable` saturation and the two `MathOverflow` returns (#647)