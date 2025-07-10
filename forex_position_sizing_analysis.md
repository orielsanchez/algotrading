# Forex Position Sizing Analysis

## Investigation Summary

After analyzing the codebase, I've identified the root cause of why forex position sizes are calculating to 0. Here's the complete breakdown:

## Position Sizing Calculation Pipeline

### 1. Signal Strength Calculation (momentum.rs:462-581)

The system calculates signal strength using Carver's -20 to +20 scale:
- Base signal comes from composite momentum score
- Gets centered around momentum threshold: `centered_momentum = base_signal - momentum_threshold`
- Scaled by 20x: `scaled_signal = centered_momentum * 20.0`
- Quality multipliers applied based on Sharpe ratio, volatility, consensus
- Final signal capped at [-20, +20]

Example for strong signal: `9.3768` (as seen in user's logs)

### 2. Volatility-Based Position Sizing (momentum.rs:584-654)

Calls `position_manager.calculate_position_size()` which currently uses a **placeholder implementation**:

```rust
// position_manager.rs:45-55
pub fn calculate_position_size(
    &self,
    _symbol: &str,
    signal_strength: f64,
    _current_price: f64,
) -> f64 {
    // For now, return a simple calculation based on signal strength
    // This maintains the basic behavior while we work on full integration
    let base_size = 1000.0; // Base position size from config
    base_size * signal_strength.abs() / 20.0 // Scale by Carver signal strength
}
```

### 3. The Math Behind Zero Position Sizes

For a signal strength of 9.3768:
- `raw_position_size = 1000.0 * 9.3768 / 20.0 = 468.84`

This should NOT be zero! The problem is elsewhere...

### 4. Forex-Specific Adjustments (momentum.rs:611-645)

For forex, the raw position size gets converted to base currency units:
- `base_currency_units = raw_position_size / price`
- For EUR.USD at 1.0850: `468.84 / 1.0850 = 432.18 units`
- Then rounded to lot sizes (1000, 10000, or 100000)
- Final size: `(432.18 / 1000.0).floor().max(1.0) * 1000.0 = 1000.0`

This should result in 1000 units, not 0!

### 5. **THE ACTUAL PROBLEM: Threshold Filter**

Found in momentum.rs:318:
```rust
if (target_position - current_position).abs() > 0.01 {
    // Only generate signal if position change > 0.01
}
```

**This 0.01 threshold is designed for stock shares, not forex units!**

## Issues Identified

### Issue 1: Inappropriate Threshold for Forex
The 0.01 threshold works for stocks (0.01 shares) but not forex:
- Forex positions are in base currency units (1000s or 10000s)
- A change of 0.01 EUR is meaningless
- Should be at least 100-1000 units for micro/mini lots

### Issue 2: Placeholder Position Sizing
The position manager uses a placeholder calculation instead of the sophisticated volatility targeting from `VolatilityTargeter`:
- Has `volatility_targeter` field but doesn't use it
- Should use Carver's formula: `(signal_strength * target_vol * portfolio_value) / (instrument_vol * price)`

### Issue 3: Config Mismatch
The config has `position_size: 200.0` but position manager uses hardcoded `1000.0`

## Test Case Calculation

For EUR.USD with signal strength 9.3768:

**Current broken path:**
1. `raw_size = 1000.0 * 9.3768 / 20.0 = 468.84`
2. `base_units = 468.84 / 1.0850 = 432.18`
3. `final_size = (432.18 / 1000.0).floor() * 1000.0 = 0 * 1000.0 = 0` ❌

**What should happen:**
1. Use proper volatility targeting
2. Apply appropriate lot sizing for forex (minimum 1000 units)
3. Use forex-appropriate threshold (e.g., 500 units minimum change)

## Recommended Fixes

### Fix 1: Forex-Aware Threshold
```rust
let min_change_threshold = match security_info.security_type {
    SecurityType::Stock => 0.01,        // 0.01 shares
    SecurityType::Forex => 500.0,       // 500 base currency units
    SecurityType::Future => 1.0,        // 1 contract
};

if (target_position - current_position).abs() > min_change_threshold {
    // Generate signal
}
```

### Fix 2: Integrate Proper Volatility Targeting
Use the `VolatilityTargeter` instead of placeholder calculation

### Fix 3: Fix Lot Sizing Logic
Ensure minimum position is 1000 units (micro lot) for forex

## Impact
This explains why forex signals show strong signal strength (9.3768) but quantity 0 - the position changes are being calculated but filtered out by an inappropriate threshold.