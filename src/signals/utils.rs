//! Shared utility functions for signal generation
//!
//! Common calculations used across all signal types to eliminate code duplication
//! and ensure consistent behavior following Carver's systematic trading principles.

use crate::market_data::TimeFrame;
use anyhow::Result;
use std::collections::HashMap;

/// Shared signal calculation utilities
pub struct SignalUtils;

impl SignalUtils {
    /// Calculate historical volatility from price data
    ///
    /// # Arguments
    /// * `prices` - Historical price data
    /// * `annualized` - Whether to annualize the volatility (assume daily data)
    ///
    /// # Returns
    /// * `f64` - Volatility as standard deviation of returns
    pub fn calculate_volatility(prices: &[f64], annualized: bool) -> Result<f64> {
        if prices.len() < 2 {
            return Err(anyhow::anyhow!(
                "Need at least 2 prices for volatility calculation"
            ));
        }

        // Calculate log returns
        let returns: Vec<f64> = prices.windows(2).map(|w| (w[1] / w[0]).ln()).collect();

        if returns.is_empty() {
            return Err(anyhow::anyhow!("No returns calculated from prices"));
        }

        // Calculate mean return
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;

        // Calculate variance
        let variance = returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / returns.len() as f64;

        let mut volatility = variance.sqrt();

        // Annualize if requested (assume daily data, 252 trading days)
        if annualized {
            volatility *= (252.0_f64).sqrt();
        }

        Ok(volatility)
    }

    /// Calculate simple returns volatility (for price differences vs log returns)
    ///
    /// # Arguments
    /// * `prices` - Historical price data
    /// * `annualized` - Whether to annualize the volatility
    ///
    /// # Returns
    /// * `f64` - Volatility as standard deviation of simple returns
    pub fn calculate_simple_volatility(prices: &[f64], annualized: bool) -> Result<f64> {
        if prices.len() < 2 {
            return Err(anyhow::anyhow!(
                "Need at least 2 prices for volatility calculation"
            ));
        }

        // Calculate simple returns
        let returns: Vec<f64> = prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();

        if returns.is_empty() {
            return Err(anyhow::anyhow!("No returns calculated from prices"));
        }

        // Calculate mean return
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;

        // Calculate variance
        let variance = returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / returns.len() as f64;

        let mut volatility = variance.sqrt();

        // Annualize if requested (assume daily data, 252 trading days)
        if annualized {
            volatility *= (252.0_f64).sqrt();
        }

        Ok(volatility)
    }

    /// Calculate percentile rank of current value vs historical values
    ///
    /// # Arguments
    /// * `current_value` - Current value to rank
    /// * `historical_values` - Historical values for comparison
    ///
    /// # Returns
    /// * `f64` - Percentile rank (0.0 to 1.0)
    pub fn calculate_percentile_rank(current_value: f64, historical_values: &[f64]) -> f64 {
        if historical_values.is_empty() {
            return 0.5; // Default to median if no history
        }

        let below_count = historical_values
            .iter()
            .filter(|&&value| value < current_value)
            .count();

        below_count as f64 / historical_values.len() as f64
    }

    /// Calculate consensus strength across multiple signal strengths
    ///
    /// # Arguments
    /// * `signal_strengths` - Vector of signal strengths from different timeframes
    ///
    /// # Returns
    /// * `f64` - Consensus strength (0.0 to 1.0)
    pub fn calculate_consensus_strength(signal_strengths: &[f64]) -> f64 {
        if signal_strengths.len() < 2 {
            return 1.0; // Single signal has perfect "consensus"
        }

        // Count signals by direction
        let positive_count = signal_strengths.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signal_strengths.iter().filter(|&&s| s < 0.0).count();
        let _neutral_count = signal_strengths.len() - positive_count - negative_count;

        let total_signals = signal_strengths.len();
        let max_directional = positive_count.max(negative_count);

        // Calculate consensus as the fraction of signals in the dominant direction
        if max_directional == 0 {
            // All neutral signals
            0.0
        } else {
            max_directional as f64 / total_signals as f64
        }
    }

    /// Calculate consensus score using categorized approach (like carry.rs)
    ///
    /// # Arguments
    /// * `signal_strengths` - Vector of signal strengths from different timeframes
    ///
    /// # Returns
    /// * `f64` - Consensus score (0.0 to 1.0)
    pub fn calculate_consensus_score(signal_strengths: &[f64]) -> f64 {
        if signal_strengths.len() < 2 {
            return 1.0; // Single signal has perfect consensus
        }

        // Count signals by direction (threshold of 1.0 for meaningful signals)
        let positive_count = signal_strengths.iter().filter(|&&s| s > 1.0).count();
        let negative_count = signal_strengths.iter().filter(|&&s| s < -1.0).count();
        let total_meaningful = positive_count + negative_count;

        if total_meaningful == 0 {
            return 0.33; // No meaningful signals
        }

        let max_directional = positive_count.max(negative_count);
        let agreement_ratio = max_directional as f64 / total_meaningful as f64;

        // Categorized consensus scoring
        if agreement_ratio >= 1.0 {
            1.0 // Perfect consensus
        } else if agreement_ratio >= 0.67 {
            0.67 // Strong majority
        } else if agreement_ratio >= 0.5 {
            0.5 // Simple majority
        } else {
            0.33 // Mixed signals
        }
    }

    /// Apply Carver-style consensus boost to signal strength
    ///
    /// # Arguments
    /// * `signal` - Base signal strength
    /// * `consensus_strength` - Consensus strength (0.0 to 1.0)
    ///
    /// # Returns
    /// * `f64` - Boosted signal strength
    pub fn apply_consensus_boost(signal: f64, consensus_strength: f64) -> f64 {
        let boost_multiplier = if consensus_strength > 0.7 {
            1.25 // 25% boost for strong consensus
        } else if consensus_strength > 0.5 {
            1.1 // 10% boost for moderate consensus
        } else {
            1.0 // No boost for weak consensus
        };

        signal * boost_multiplier
    }

    /// Clamp signal to Carver's -20 to +20 range
    ///
    /// # Arguments
    /// * `signal` - Raw signal strength
    ///
    /// # Returns
    /// * `f64` - Signal clamped to Carver range
    pub fn clamp_to_carver_range(signal: f64) -> f64 {
        signal.clamp(-20.0, 20.0)
    }

    /// Calculate quality multiplier based on signal characteristics
    ///
    /// # Arguments
    /// * `signal_strength` - Raw signal strength
    /// * `volatility` - Current volatility level
    /// * `percentile_rank` - Percentile rank of signal vs history
    ///
    /// # Returns
    /// * `f64` - Quality multiplier (0.5 to 1.5)
    pub fn calculate_quality_multiplier(
        signal_strength: f64,
        volatility: f64,
        percentile_rank: f64,
    ) -> f64 {
        // Base quality starts at 1.0
        let mut quality: f64 = 1.0;

        // Boost for extreme percentile ranks (very unusual signals)
        if percentile_rank > 0.9 || percentile_rank < 0.1 {
            quality *= 1.2;
        } else if percentile_rank > 0.8 || percentile_rank < 0.2 {
            quality *= 1.1;
        }

        // Penalize very high volatility (regime uncertainty)
        if volatility > 0.5 {
            quality *= 0.8; // High volatility reduces confidence
        } else if volatility > 0.3 {
            quality *= 0.9; // Moderate volatility slight reduction
        }

        // Boost for strong signals
        if signal_strength.abs() > 15.0 {
            quality *= 1.2;
        } else if signal_strength.abs() > 10.0 {
            quality *= 1.1;
        }

        // Clamp quality multiplier to reasonable range
        quality.clamp(0.5, 1.5)
    }

    /// Calculate regime adjustment based on volatility level
    ///
    /// # Arguments
    /// * `current_volatility` - Current volatility
    /// * `average_volatility` - Historical average volatility
    ///
    /// # Returns
    /// * `f64` - Regime adjustment factor (0.5 to 1.2)
    pub fn calculate_regime_adjustment(current_volatility: f64, average_volatility: f64) -> f64 {
        if average_volatility <= 0.0 {
            return 1.0; // No adjustment if no historical data
        }

        let volatility_ratio = current_volatility / average_volatility;

        // Reduce signal strength during high volatility regimes
        if volatility_ratio > 2.0 {
            0.5 // Very high volatility - strong dampening
        } else if volatility_ratio > 1.5 {
            0.7 // High volatility - moderate dampening
        } else if volatility_ratio < 0.5 {
            1.2 // Low volatility - slight boost
        } else {
            1.0 // Normal volatility - no adjustment
        }
    }

    /// Calculate composite signal from multiple timeframe signals
    ///
    /// # Arguments
    /// * `timeframe_signals` - HashMap of timeframe to signal strength
    /// * `timeframe_weights` - Optional weights for each timeframe
    ///
    /// # Returns
    /// * `f64` - Weighted composite signal strength
    pub fn calculate_composite_signal(
        timeframe_signals: &HashMap<TimeFrame, f64>,
        timeframe_weights: Option<&HashMap<TimeFrame, f64>>,
    ) -> f64 {
        if timeframe_signals.is_empty() {
            return 0.0;
        }

        let default_weights = Self::default_timeframe_weights();
        let weights = timeframe_weights.unwrap_or(&default_weights);

        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;

        for (timeframe, signal_strength) in timeframe_signals {
            let weight = weights.get(timeframe).copied().unwrap_or(0.25); // Equal weight default
            weighted_sum += signal_strength * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            Self::clamp_to_carver_range(weighted_sum / total_weight)
        } else {
            0.0
        }
    }

    /// Get default timeframe weights following Carver's approach
    ///
    /// # Returns
    /// * `HashMap<TimeFrame, f64>` - Default weights for each timeframe
    pub fn default_timeframe_weights() -> HashMap<TimeFrame, f64> {
        let mut weights = HashMap::new();
        weights.insert(TimeFrame::Days2_8, 0.4); // Short-term gets highest weight
        weights.insert(TimeFrame::Days4_16, 0.3); // Medium-short term
        weights.insert(TimeFrame::Days8_32, 0.2); // Medium-long term
        weights.insert(TimeFrame::Days16_64, 0.1); // Long-term gets lowest weight
        weights
    }

    /// Calculate Sharpe-like ratio for signal quality assessment
    ///
    /// # Arguments
    /// * `signal_strength` - Current signal strength
    /// * `volatility` - Signal volatility or uncertainty
    ///
    /// # Returns
    /// * `f64` - Sharpe-like ratio (signal/volatility)
    pub fn calculate_signal_sharpe(signal_strength: f64, volatility: f64) -> f64 {
        if volatility <= 0.0 {
            return signal_strength.signum() * 20.0; // Max signal if no volatility
        }

        let sharpe = signal_strength / volatility;
        Self::clamp_to_carver_range(sharpe)
    }

    /// Validate signal data meets quality standards
    ///
    /// # Arguments
    /// * `signal_strength` - Signal strength to validate
    /// * `percentile_rank` - Percentile rank to validate
    ///
    /// # Returns
    /// * `Result<()>` - Ok if valid, Error if invalid
    pub fn validate_signal_data(signal_strength: f64, percentile_rank: f64) -> Result<()> {
        if !signal_strength.is_finite() {
            return Err(anyhow::anyhow!(
                "Signal strength must be finite, got: {}",
                signal_strength
            ));
        }

        if percentile_rank < 0.0 || percentile_rank > 1.0 {
            return Err(anyhow::anyhow!(
                "Percentile rank must be between 0.0 and 1.0, got: {}",
                percentile_rank
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_calculation() {
        let prices = vec![100.0, 102.0, 101.0, 103.0, 102.5];
        let volatility = SignalUtils::calculate_volatility(&prices, false).unwrap();
        assert!(volatility > 0.0);
        assert!(volatility < 1.0); // Should be reasonable for this data
    }

    #[test]
    fn test_percentile_rank() {
        let historical = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        assert_eq!(
            SignalUtils::calculate_percentile_rank(3.0, &historical),
            0.4
        );
        assert_eq!(
            SignalUtils::calculate_percentile_rank(1.0, &historical),
            0.0
        );
        assert_eq!(
            SignalUtils::calculate_percentile_rank(6.0, &historical),
            1.0
        );
    }

    #[test]
    fn test_consensus_strength() {
        // All positive signals
        let signals = vec![5.0, 3.0, 7.0];
        assert_eq!(SignalUtils::calculate_consensus_strength(&signals), 1.0);

        // Mixed signals
        let signals = vec![5.0, -3.0, 7.0];
        assert!((SignalUtils::calculate_consensus_strength(&signals) - 0.667).abs() < 0.001);

        // All negative signals
        let signals = vec![-5.0, -3.0, -7.0];
        assert_eq!(SignalUtils::calculate_consensus_strength(&signals), 1.0);
    }

    #[test]
    fn test_consensus_boost() {
        let signal = 10.0;

        // Strong consensus (>0.7)
        assert_eq!(SignalUtils::apply_consensus_boost(signal, 0.8), 12.5);

        // Moderate consensus (>0.5)
        assert_eq!(SignalUtils::apply_consensus_boost(signal, 0.6), 11.0);

        // Weak consensus
        assert_eq!(SignalUtils::apply_consensus_boost(signal, 0.4), 10.0);
    }

    #[test]
    fn test_carver_range_clamping() {
        assert_eq!(SignalUtils::clamp_to_carver_range(25.0), 20.0);
        assert_eq!(SignalUtils::clamp_to_carver_range(-25.0), -20.0);
        assert_eq!(SignalUtils::clamp_to_carver_range(15.0), 15.0);
    }

    #[test]
    fn test_regime_adjustment() {
        // High volatility should dampen signals
        assert!(SignalUtils::calculate_regime_adjustment(0.4, 0.2) < 1.0);

        // Low volatility should boost signals (ratio < 0.5)
        assert!(SignalUtils::calculate_regime_adjustment(0.08, 0.2) > 1.0);

        // Boundary case - exactly 0.5 ratio should not boost
        assert_eq!(SignalUtils::calculate_regime_adjustment(0.1, 0.2), 1.0);

        // Normal volatility should not adjust
        assert_eq!(SignalUtils::calculate_regime_adjustment(0.2, 0.2), 1.0);
    }
}
