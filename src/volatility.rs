use crate::config::RiskConfig;
use log::{debug, info};
use std::collections::HashMap;

/// Exponentially Weighted Moving Average (EWMA) volatility calculation
/// Following Carver's approach for volatility targeting
#[derive(Debug, Clone)]
pub struct VolatilityCalculator {
    /// Half-life for EWMA calculation (typically 32-64 days)
    pub half_life: f64,
    /// Smoothing factor (lambda) derived from half-life
    pub lambda: f64,
    /// Annualization factor (typically 252 for daily data)
    pub annualization_factor: f64,
    /// Historical price data for each instrument
    price_history: HashMap<String, Vec<f64>>,
    /// Current volatility estimates
    volatility_estimates: HashMap<String, f64>,
    /// Return history for volatility calculation
    return_history: HashMap<String, Vec<f64>>,
}

impl VolatilityCalculator {
    /// Create new volatility calculator with specified half-life
    pub fn new(half_life: f64, annualization_factor: f64) -> Self {
        let lambda = (-1.0 / half_life).exp();
        
        info!("Initializing volatility calculator with half-life: {:.1} days, lambda: {:.4}", 
              half_life, lambda);
        
        Self {
            half_life,
            lambda,
            annualization_factor,
            price_history: HashMap::new(),
            volatility_estimates: HashMap::new(),
            return_history: HashMap::new(),
        }
    }
    
    /// Update price data and calculate new volatility estimate
    pub fn update_price(&mut self, symbol: &str, price: f64) {
        // Initialize if first price for this symbol
        if !self.price_history.contains_key(symbol) {
            self.price_history.insert(symbol.to_string(), Vec::new());
            self.return_history.insert(symbol.to_string(), Vec::new());
            self.volatility_estimates.insert(symbol.to_string(), 0.0);
        }
        
        let return_rate = {
            let prices = self.price_history.get_mut(symbol).unwrap();
            let returns = self.return_history.get_mut(symbol).unwrap();
            
            // Add new price
            prices.push(price);
            
            // Calculate return if we have previous price
            let return_rate = if prices.len() >= 2 {
                let prev_price = prices[prices.len() - 2];
                let return_rate = (price / prev_price).ln();
                returns.push(return_rate);
                Some(return_rate)
            } else {
                None
            };
            
            // Keep only recent history (e.g., last 252 days)
            let max_history = (self.half_life * 4.0) as usize;
            if prices.len() > max_history {
                prices.drain(0..prices.len() - max_history);
            }
            if returns.len() > max_history {
                returns.drain(0..returns.len() - max_history);
            }
            
            return_rate
        };
        
        // Update volatility estimate using EWMA
        if let Some(rate) = return_rate {
            self.update_volatility_estimate(symbol, rate);
        }
    }
    
    /// Update EWMA volatility estimate
    fn update_volatility_estimate(&mut self, symbol: &str, new_return: f64) {
        let returns_len = self.return_history.get(symbol).unwrap().len();
        let current_vol = *self.volatility_estimates.get(symbol).unwrap();
        
        if returns_len == 1 {
            // First return - use squared return as initial variance
            let initial_variance = new_return * new_return;
            let initial_vol = (initial_variance * self.annualization_factor).sqrt();
            self.volatility_estimates.insert(symbol.to_string(), initial_vol);
            
            debug!("Initial volatility estimate for {}: {:.4}", symbol, initial_vol);
        } else {
            // EWMA update: new_variance = lambda * old_variance + (1-lambda) * new_return^2
            let old_variance = (current_vol / self.annualization_factor.sqrt()).powi(2);
            let new_variance = self.lambda * old_variance + (1.0 - self.lambda) * new_return * new_return;
            let new_vol = (new_variance * self.annualization_factor).sqrt();
            
            self.volatility_estimates.insert(symbol.to_string(), new_vol);
            
            debug!("Updated volatility for {}: {:.4} (from {:.4})", symbol, new_vol, current_vol);
        }
    }
    
    /// Get current volatility estimate for an instrument
    pub fn get_volatility(&self, symbol: &str) -> Option<f64> {
        self.volatility_estimates.get(symbol).copied()
    }
    
    /// Get all current volatility estimates
    pub fn get_all_volatilities(&self) -> &HashMap<String, f64> {
        &self.volatility_estimates
    }
    
    /// Check if we have sufficient data for reliable volatility estimate
    pub fn has_sufficient_data(&self, symbol: &str) -> bool {
        if let Some(returns) = self.return_history.get(symbol) {
            returns.len() >= 10 // Minimum 10 returns for reasonable estimate
        } else {
            false
        }
    }
}

/// Portfolio volatility targeting following Carver's approach
#[derive(Debug, Clone)]
pub struct VolatilityTargeter {
    /// Target portfolio volatility (typically 0.25 = 25% annualized)
    pub target_volatility: f64,
    /// Volatility calculator
    pub volatility_calc: VolatilityCalculator,
    /// Risk configuration
    pub risk_config: RiskConfig,
}

impl VolatilityTargeter {
    /// Create new volatility targeter
    pub fn new(target_volatility: f64, risk_config: RiskConfig) -> Self {
        let volatility_calc = VolatilityCalculator::new(32.0, 252.0); // 32-day half-life, 252 trading days
        
        info!("Initializing volatility targeter with target: {:.1}%", target_volatility * 100.0);
        
        Self {
            target_volatility,
            volatility_calc,
            risk_config,
        }
    }
    
    /// Update price data for all instruments
    pub fn update_prices(&mut self, prices: &HashMap<String, f64>) {
        for (symbol, price) in prices {
            self.volatility_calc.update_price(symbol, *price);
        }
    }
    
    /// Calculate position size based on volatility targeting
    pub fn calculate_position_size(
        &self,
        symbol: &str,
        signal_strength: f64,
        portfolio_value: f64,
        price: f64,
    ) -> f64 {
        // Get instrument volatility
        let instrument_vol = match self.volatility_calc.get_volatility(symbol) {
            Some(vol) if vol > 0.0 => vol,
            _ => {
                debug!("No volatility data for {}, using default 20%", symbol);
                0.20 // Default 20% volatility
            }
        };
        
        // Carver's position sizing formula:
        // position_size = (signal_strength * target_vol * portfolio_value) / (instrument_vol * price)
        let position_size = (signal_strength * self.target_volatility * portfolio_value) / (instrument_vol * price);
        
        debug!(
            "Volatility-based position sizing for {}: signal={:.2}, target_vol={:.3}, portfolio=${:.0}, instr_vol={:.3}, price={:.4} -> size={:.0}",
            symbol, signal_strength, self.target_volatility, portfolio_value, instrument_vol, price, position_size
        );
        
        position_size
    }
    
    /// Calculate instrument diversification multiplier (IDM)
    /// This accounts for the fact that not all instruments trade at the same time
    pub fn calculate_idm(&self, num_instruments: usize) -> f64 {
        // Simple IDM based on Carver's approach
        // More sophisticated versions would use correlation matrix
        let base_idm = (num_instruments as f64).sqrt();
        
        // Cap IDM to reasonable range
        base_idm.min(2.5).max(1.0)
    }
    
    /// Calculate forecast diversification multiplier (FDM)
    /// This accounts for combining multiple trading signals
    pub fn calculate_fdm(&self, num_signals: usize) -> f64 {
        // Simple FDM - in practice this would use signal correlation
        let base_fdm = (num_signals as f64).sqrt();
        
        // Cap FDM to reasonable range
        base_fdm.min(2.0).max(1.0)
    }
    
    /// Get portfolio volatility estimate
    pub fn get_portfolio_volatility(&self, positions: &HashMap<String, f64>) -> f64 {
        // Simplified portfolio volatility calculation
        // In practice, this would use full covariance matrix
        let mut weighted_vol = 0.0;
        let mut total_weight = 0.0;
        
        for (symbol, weight) in positions {
            if let Some(vol) = self.volatility_calc.get_volatility(symbol) {
                weighted_vol += weight.abs() * vol;
                total_weight += weight.abs();
            }
        }
        
        if total_weight > 0.0 {
            weighted_vol / total_weight
        } else {
            0.0
        }
    }
    
    /// Check if portfolio volatility is within target range
    pub fn is_volatility_on_target(&self, positions: &HashMap<String, f64>) -> bool {
        let current_vol = self.get_portfolio_volatility(positions);
        let tolerance = 0.05; // 5% tolerance
        
        (current_vol - self.target_volatility).abs() <= tolerance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RiskConfig;
    
    #[test]
    fn test_volatility_calculator() {
        let mut calc = VolatilityCalculator::new(32.0, 252.0);
        
        // Test with simple price series
        let prices = vec![100.0, 101.0, 99.0, 102.0, 98.0];
        for price in prices {
            calc.update_price("TEST", price);
        }
        
        assert!(calc.has_sufficient_data("TEST"));
        assert!(calc.get_volatility("TEST").is_some());
        assert!(calc.get_volatility("TEST").unwrap() > 0.0);
    }
    
    #[test]
    fn test_volatility_targeter() {
        let risk_config = RiskConfig::default();
        let mut targeter = VolatilityTargeter::new(0.25, risk_config);
        
        // Update with some prices
        let mut prices = HashMap::new();
        prices.insert("EURUSD".to_string(), 1.0850);
        targeter.update_prices(&prices);
        
        // Test position sizing
        let position_size = targeter.calculate_position_size("EURUSD", 1.0, 10000.0, 1.0850);
        assert!(position_size > 0.0);
    }
}