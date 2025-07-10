//! Signal Coordinator
//!
//! Unified signal coordination system that replaces manual signal combination
//! in momentum.rs. Provides centralized signal weighting and combination following
//! Carver's systematic trading framework.

use super::core::{CombinedSignals, SignalCore, SignalGenerator, SignalType, SignalWeights};
use super::utils::SignalUtils;
use crate::market_data::{MarketDataHandler, TimeFrame};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Configuration for signal coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    pub signal_weights: SignalWeights,
    pub consensus_threshold: f64, // Minimum consensus for signal boosting
    pub quality_filter_threshold: f64, // Minimum quality for signal inclusion
    pub enable_cross_validation: bool, // Whether to validate signals against each other
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            signal_weights: SignalWeights::default(),
            consensus_threshold: 0.67,
            quality_filter_threshold: 1.0,
            enable_cross_validation: true,
        }
    }
}

/// Signal coordination engine
pub struct SignalCoordinator {
    config: CoordinatorConfig,
}

impl SignalCoordinator {
    /// Create new signal coordinator with default configuration
    pub fn new() -> Self {
        Self {
            config: CoordinatorConfig::default(),
        }
    }

    /// Create signal coordinator with custom configuration
    pub fn with_config(config: CoordinatorConfig) -> Result<Self> {
        // Validate configuration
        config.signal_weights.validate()?;

        if config.consensus_threshold < 0.0 || config.consensus_threshold > 1.0 {
            return Err(anyhow::anyhow!(
                "Consensus threshold must be between 0.0 and 1.0, got: {}",
                config.consensus_threshold
            ));
        }

        Ok(Self { config })
    }

    /// Combine signals from multiple generators into unified signal
    ///
    /// # Arguments
    /// * `momentum_signal` - Optional momentum signal core
    /// * `breakout_signal` - Optional breakout signal core  
    /// * `carry_signal` - Optional carry signal core
    /// * `mean_reversion_signal` - Optional mean reversion signal core
    ///
    /// # Returns
    /// * `CombinedSignals` - Unified signal combination
    pub fn combine_signals(
        &self,
        momentum_signal: Option<SignalCore>,
        breakout_signal: Option<SignalCore>,
        carry_signal: Option<SignalCore>,
        mean_reversion_signal: Option<SignalCore>,
    ) -> CombinedSignals {
        // Filter signals based on quality threshold
        let momentum = self.filter_signal_quality(momentum_signal);
        let breakout = self.filter_signal_quality(breakout_signal);
        let carry = self.filter_signal_quality(carry_signal);
        let mean_reversion = self.filter_signal_quality(mean_reversion_signal);

        // Calculate weighted composite signal
        let composite_strength =
            self.calculate_weighted_composite(&momentum, &breakout, &carry, &mean_reversion);

        // Determine dominant signal type
        let dominant_signal =
            self.find_dominant_signal(&momentum, &breakout, &carry, &mean_reversion);

        // Calculate cross-signal agreement
        let agreement_score = if self.config.enable_cross_validation {
            self.calculate_agreement_score(&momentum, &breakout, &carry, &mean_reversion)
        } else {
            1.0 // No cross-validation, assume perfect agreement
        };

        // Apply consensus boost if agreement is strong
        let final_composite = if agreement_score >= self.config.consensus_threshold {
            SignalUtils::apply_consensus_boost(composite_strength, agreement_score)
        } else {
            composite_strength
        };

        CombinedSignals {
            momentum,
            breakout,
            carry,
            mean_reversion,
            composite_strength: SignalUtils::clamp_to_carver_range(final_composite),
            dominant_signal,
            agreement_score,
        }
    }

    /// Calculate signals for all generators and combine them
    ///
    /// # Arguments
    /// * `symbol` - Trading symbol
    /// * `momentum_gen` - Momentum signal generator
    /// * `breakout_gen` - Breakout signal generator
    /// * `carry_gen` - Carry signal generator
    /// * `market_data` - Market data handler
    ///
    /// # Returns
    /// * `Result<CombinedSignals>` - Combined signals or error
    pub fn calculate_all_signals<M, B, C>(
        &self,
        symbol: &str,
        momentum_gen: &M,
        breakout_gen: &B,
        carry_gen: &C,
        market_data: &MarketDataHandler,
    ) -> Result<CombinedSignals>
    where
        M: SignalGenerator,
        B: SignalGenerator,
        C: SignalGenerator,
    {
        // Calculate individual signals (using multi-timeframe methods)
        let momentum_metrics = momentum_gen.calculate_multi_timeframe(symbol, market_data)?;
        let breakout_metrics = breakout_gen.calculate_multi_timeframe(symbol, market_data)?;
        let carry_metrics = carry_gen.calculate_multi_timeframe(symbol, market_data)?;

        // Extract strongest signals or composite signals from multi-timeframe analysis
        let momentum_signal = momentum_metrics.map(|m| {
            // This would need to be implemented based on the specific metrics type
            // For now, create a placeholder SignalCore
            self.extract_signal_from_metrics(m, SignalType::Momentum)
        });

        let breakout_signal =
            breakout_metrics.map(|m| self.extract_signal_from_metrics(m, SignalType::Breakout));

        let carry_signal =
            carry_metrics.map(|m| self.extract_signal_from_metrics(m, SignalType::Carry));

        // Combine all signals
        Ok(self.combine_signals(
            momentum_signal,
            breakout_signal,
            carry_signal,
            None, // No mean reversion generator provided
        ))
    }

    /// Update signal weights for rebalancing
    ///
    /// # Arguments
    /// * `new_weights` - New signal weights
    ///
    /// # Returns
    /// * `Result<()>` - Success or validation error
    pub fn update_weights(&mut self, new_weights: SignalWeights) -> Result<()> {
        new_weights.validate()?;
        self.config.signal_weights = new_weights;
        Ok(())
    }

    /// Get current configuration
    pub fn config(&self) -> &CoordinatorConfig {
        &self.config
    }

    // Private helper methods

    /// Filter signal based on quality threshold
    fn filter_signal_quality(&self, signal: Option<SignalCore>) -> Option<SignalCore> {
        signal.filter(|s| s.signal_strength.abs() >= self.config.quality_filter_threshold)
    }

    /// Calculate weighted composite signal strength
    fn calculate_weighted_composite(
        &self,
        momentum: &Option<SignalCore>,
        breakout: &Option<SignalCore>,
        carry: &Option<SignalCore>,
        mean_reversion: &Option<SignalCore>,
    ) -> f64 {
        let weights = &self.config.signal_weights;
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;

        // Add momentum component
        if let Some(signal) = momentum {
            weighted_sum += signal.signal_strength * weights.momentum;
            total_weight += weights.momentum;
        }

        // Add breakout component
        if let Some(signal) = breakout {
            weighted_sum += signal.signal_strength * weights.breakout;
            total_weight += weights.breakout;
        }

        // Add carry component
        if let Some(signal) = carry {
            weighted_sum += signal.signal_strength * weights.carry;
            total_weight += weights.carry;
        }

        // Add mean reversion component
        if let Some(signal) = mean_reversion {
            weighted_sum += signal.signal_strength * weights.mean_reversion;
            total_weight += weights.mean_reversion;
        }

        // Normalize by actual weight used (in case some signals are missing)
        if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        }
    }

    /// Find the signal type with strongest absolute strength
    fn find_dominant_signal(
        &self,
        momentum: &Option<SignalCore>,
        breakout: &Option<SignalCore>,
        carry: &Option<SignalCore>,
        mean_reversion: &Option<SignalCore>,
    ) -> Option<SignalType> {
        let mut max_strength = 0.0;
        let mut dominant_type = None;

        // Check momentum
        if let Some(signal) = momentum {
            let abs_strength = signal.signal_strength.abs();
            if abs_strength > max_strength {
                max_strength = abs_strength;
                dominant_type = Some(SignalType::Momentum);
            }
        }

        // Check breakout
        if let Some(signal) = breakout {
            let abs_strength = signal.signal_strength.abs();
            if abs_strength > max_strength {
                max_strength = abs_strength;
                dominant_type = Some(SignalType::Breakout);
            }
        }

        // Check carry
        if let Some(signal) = carry {
            let abs_strength = signal.signal_strength.abs();
            if abs_strength > max_strength {
                max_strength = abs_strength;
                dominant_type = Some(SignalType::Carry);
            }
        }

        // Check mean reversion
        if let Some(signal) = mean_reversion {
            let abs_strength = signal.signal_strength.abs();
            if abs_strength > max_strength {
                dominant_type = Some(SignalType::MeanReversion);
            }
        }

        dominant_type
    }

    /// Calculate agreement score across all active signals
    fn calculate_agreement_score(
        &self,
        momentum: &Option<SignalCore>,
        breakout: &Option<SignalCore>,
        carry: &Option<SignalCore>,
        mean_reversion: &Option<SignalCore>,
    ) -> f64 {
        let signal_strengths: Vec<f64> = [momentum, breakout, carry, mean_reversion]
            .iter()
            .filter_map(|opt| opt.as_ref())
            .map(|signal| signal.signal_strength)
            .collect();

        if signal_strengths.len() < 2 {
            return 1.0; // Single or no signals have perfect "agreement"
        }

        SignalUtils::calculate_consensus_strength(&signal_strengths)
    }

    /// Extract SignalCore from metrics (placeholder implementation)
    ///
    /// This is a placeholder that would need to be implemented based on the
    /// specific metrics types returned by each signal generator.
    fn extract_signal_from_metrics<T>(&self, _metrics: T, signal_type: SignalType) -> SignalCore {
        // TODO: This needs to be implemented based on actual metrics types
        // For now, return a placeholder
        SignalCore::new(
            "PLACEHOLDER".to_string(),
            TimeFrame::Days4_16,
            0.0,
            signal_type,
            0.5,
            0.0,
            crate::signals::core::SignalQuality::Low,
        )
    }
}

impl Default for SignalCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for signal coordinator configuration
pub struct CoordinatorBuilder {
    config: CoordinatorConfig,
}

impl CoordinatorBuilder {
    /// Create new coordinator builder
    pub fn new() -> Self {
        Self {
            config: CoordinatorConfig::default(),
        }
    }

    /// Set signal weights
    pub fn with_weights(mut self, weights: SignalWeights) -> Self {
        self.config.signal_weights = weights;
        self
    }

    /// Set consensus threshold
    pub fn with_consensus_threshold(mut self, threshold: f64) -> Self {
        self.config.consensus_threshold = threshold;
        self
    }

    /// Set quality filter threshold
    pub fn with_quality_threshold(mut self, threshold: f64) -> Self {
        self.config.quality_filter_threshold = threshold;
        self
    }

    /// Enable or disable cross-validation
    pub fn with_cross_validation(mut self, enabled: bool) -> Self {
        self.config.enable_cross_validation = enabled;
        self
    }

    /// Build the signal coordinator
    pub fn build(self) -> Result<SignalCoordinator> {
        SignalCoordinator::with_config(self.config)
    }
}

impl Default for CoordinatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::TimeFrame;
    use crate::signals::core::{SignalQuality, SignalType};

    fn create_test_signal(strength: f64, signal_type: SignalType) -> SignalCore {
        SignalCore::new(
            "TEST".to_string(),
            TimeFrame::Days4_16,
            strength,
            signal_type,
            0.5,
            strength,
            SignalQuality::Medium,
        )
    }

    #[test]
    fn test_signal_combination() {
        let coordinator = SignalCoordinator::new();

        let momentum = Some(create_test_signal(10.0, SignalType::Momentum));
        let breakout = Some(create_test_signal(5.0, SignalType::Breakout));
        let carry = Some(create_test_signal(-2.0, SignalType::Carry));

        let combined = coordinator.combine_signals(momentum, breakout, carry, None);

        // Should have positive composite (momentum dominates)
        assert!(combined.composite_strength > 0.0);
        assert_eq!(combined.dominant_signal, Some(SignalType::Momentum));
        assert!(combined.has_actionable_signals());
    }

    #[test]
    fn test_quality_filtering() {
        let mut config = CoordinatorConfig::default();
        config.quality_filter_threshold = 5.0; // High threshold

        let coordinator = SignalCoordinator::with_config(config).unwrap();

        let weak_signal = Some(create_test_signal(2.0, SignalType::Momentum)); // Below threshold
        let strong_signal = Some(create_test_signal(8.0, SignalType::Breakout)); // Above threshold

        let combined = coordinator.combine_signals(weak_signal, strong_signal, None, None);

        // Weak signal should be filtered out
        assert!(combined.momentum.is_none());
        assert!(combined.breakout.is_some());
        assert_eq!(combined.dominant_signal, Some(SignalType::Breakout));
    }

    #[test]
    fn test_consensus_boost() {
        let coordinator = SignalCoordinator::new();

        // All signals agreeing (positive direction)
        let momentum = Some(create_test_signal(8.0, SignalType::Momentum));
        let breakout = Some(create_test_signal(6.0, SignalType::Breakout));
        let carry = Some(create_test_signal(4.0, SignalType::Carry));

        let combined = coordinator.combine_signals(momentum, breakout, carry, None);

        // Should have high agreement score and boosted signal
        assert!(combined.agreement_score > 0.9);
        // Composite should be boosted beyond simple weighted average
        assert!(combined.composite_strength > 5.0);
    }

    #[test]
    fn test_coordinator_builder() {
        let weights = SignalWeights {
            momentum: 0.6,
            breakout: 0.3,
            carry: 0.1,
            mean_reversion: 0.0,
        };

        let coordinator = CoordinatorBuilder::new()
            .with_weights(weights)
            .with_consensus_threshold(0.8)
            .with_quality_threshold(2.0)
            .build()
            .unwrap();

        assert_eq!(coordinator.config().signal_weights.momentum, 0.6);
        assert_eq!(coordinator.config().consensus_threshold, 0.8);
        assert_eq!(coordinator.config().quality_filter_threshold, 2.0);
    }
}
