// Position Management - Extracted from momentum.rs
// Handles position tracking, sizing, and portfolio logic

use crate::config::RiskConfig;
use crate::volatility::VolatilityTargeter;
use std::collections::HashMap;

/// Manages position tracking and sizing calculations
#[derive(Debug)]
pub struct PositionManager {
    /// Current positions by symbol
    current_positions: HashMap<String, f64>,
    /// Volatility-based position sizing
    volatility_targeter: VolatilityTargeter,
}

impl PositionManager {
    /// Create new position manager with risk configuration
    pub fn new(risk_config: RiskConfig) -> Self {
        // Initialize volatility targeter with 25% annual target
        let volatility_targeter = VolatilityTargeter::new(0.25, risk_config);

        Self {
            current_positions: HashMap::new(),
            volatility_targeter,
        }
    }

    /// Update position for a symbol
    pub fn update_position(&mut self, symbol: &str, quantity: f64) {
        self.current_positions.insert(symbol.to_string(), quantity);
    }

    /// Get current positions (read-only access)
    pub fn get_positions(&self) -> &HashMap<String, f64> {
        &self.current_positions
    }

    /// Get current position for a specific symbol
    pub fn get_position(&self, symbol: &str) -> f64 {
        self.current_positions.get(symbol).copied().unwrap_or(0.0)
    }

    /// Calculate volatility-based position size for a symbol
    pub fn calculate_position_size(
        &self,
        symbol: &str,
        signal_strength: f64,
        current_price: f64,
        portfolio_value: f64,
    ) -> f64 {
        // Use the volatility targeter for proper position sizing
        // This implements Carver's volatility targeting methodology
        self.volatility_targeter.calculate_position_size(
            symbol,
            signal_strength,
            portfolio_value,
            current_price,
        )
    }

    /// Update prices for volatility calculations
    pub fn update_prices(&mut self, prices: &HashMap<String, f64>) {
        self.volatility_targeter.update_prices(prices);
    }

    /// Remove position (for securities dropped from universe)
    pub fn remove_position(&mut self, symbol: &str) -> Option<f64> {
        self.current_positions.remove(symbol)
    }

    /// Get all symbols with positions
    pub fn get_positioned_symbols(&self) -> Vec<String> {
        self.current_positions
            .iter()
            .filter(|(_, quantity)| **quantity != 0.0)
            .map(|(symbol, _)| symbol.clone())
            .collect()
    }

    /// Clear all positions (for testing/reset)
    pub fn clear_positions(&mut self) {
        self.current_positions.clear();
    }
}
