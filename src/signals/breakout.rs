//! Breakout Signal Generator
//!
//! Implements the SignalGenerator trait for breakout-based trading signals.
//! Provides unified interface for existing BreakoutCalculator functionality.

use crate::breakout::{BreakoutCalculator, BreakoutMetrics, BreakoutSignal, BreakoutType};
use crate::market_data::{MarketDataHandler, TimeFrame};
use crate::signals::core::{SignalCore, SignalGenerator, SignalQuality, SignalType};
use crate::signals::utils::SignalUtils;
use anyhow::Result;

/// Breakout signal generator implementing unified SignalGenerator interface
#[derive(Debug, Clone)]
pub struct BreakoutSignalGenerator {
    calculator: BreakoutCalculator,
}

impl BreakoutSignalGenerator {
    /// Create new breakout signal generator with default settings
    pub fn new() -> Self {
        Self {
            calculator: BreakoutCalculator::new(),
        }
    }

    /// Create breakout signal generator with custom calculator
    pub fn with_calculator(calculator: BreakoutCalculator) -> Self {
        Self { calculator }
    }
}

impl SignalGenerator for BreakoutSignalGenerator {
    type Signal = BreakoutSignal;
    type Metrics = BreakoutMetrics;

    fn calculate_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Signal>> {
        Ok(self
            .calculator
            .calculate_breakout_signal(symbol, timeframe, market_data))
    }

    fn calculate_multi_timeframe(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Metrics>> {
        Ok(self
            .calculator
            .calculate_multi_timeframe_breakout(symbol, market_data))
    }

    fn extract_signal_strength(&self, signal: &Self::Signal) -> f64 {
        signal.signal_strength
    }

    fn to_signal_core(&self, signal: &Self::Signal) -> SignalCore {
        // Calculate Carver-compliant signal strength using improved logic
        let carver_strength = if signal.signal_strength == 0.0 {
            // Calculate strength based on breakout characteristics and volatility
            let base_strength = match signal.breakout_type {
                BreakoutType::UpBreakout => {
                    // Stronger signals for higher percentile ranks
                    let strength = signal.percentile_rank * 20.0;
                    strength * (1.0 + signal.volatility_normalized) // Volatility boost
                }
                BreakoutType::DownBreakout => {
                    // Stronger negative signals for lower percentile ranks
                    let strength = (1.0 - signal.percentile_rank) * -20.0;
                    strength * (1.0 + signal.volatility_normalized) // Volatility boost
                }
                BreakoutType::NoBreakout => 0.0,
            };

            // Apply Carver range clamping using shared utility
            SignalUtils::clamp_to_carver_range(base_strength)
        } else {
            SignalUtils::clamp_to_carver_range(signal.signal_strength)
        };

        // Determine signal quality based on strength and characteristics
        let quality = self.assess_signal_quality(signal, carver_strength);

        SignalCore::new(
            signal.symbol.clone(),
            signal.timeframe,
            carver_strength,
            SignalType::Breakout,
            signal.percentile_rank,
            signal.volatility_normalized,
            quality,
        )
    }

    fn signal_type(&self) -> SignalType {
        SignalType::Breakout
    }
}

/// Private implementation methods
impl BreakoutSignalGenerator {
    /// Assess signal quality based on strength and breakout characteristics
    fn assess_signal_quality(
        &self,
        signal: &BreakoutSignal,
        carver_strength: f64,
    ) -> SignalQuality {
        let strength_abs = carver_strength.abs();

        // High quality: Strong signals with good volatility normalization
        if strength_abs > 15.0 && signal.volatility_normalized > 0.7 {
            return SignalQuality::High;
        }

        // Medium quality: Moderate signals
        if strength_abs > 5.0 && signal.volatility_normalized > 0.3 {
            return SignalQuality::Medium;
        }

        // Low quality: Weak signals but still actionable
        if strength_abs > 1.0 {
            return SignalQuality::Low;
        }

        // Filtered: Very weak or no breakout
        SignalQuality::Filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::breakout::{BreakoutCalculator, BreakoutType};
    use crate::market_data::{MarketDataHandler, TimeFrame};

    /// Helper function to create test market data handler
    fn create_test_market_data() -> MarketDataHandler {
        use chrono::{Duration, Utc};
        use time;

        let mut handler = MarketDataHandler::new();

        // Register the symbol first
        handler.register_symbol(1, "AAPL".to_string());

        // Add test price data for AAPL
        let test_prices = vec![
            100.0, 102.0, 101.0, 103.0, 105.0, 104.0, 106.0, 108.0, 107.0, 110.0, 112.0, 111.0,
            113.0, 115.0, 114.0, 116.0, 118.0, 117.0, 119.0, 120.0,
        ];

        let now = Utc::now();
        for (i, price) in test_prices.iter().enumerate() {
            let timestamp = now - Duration::days(test_prices.len() as i64 - i as i64);
            let offset_datetime = time::OffsetDateTime::from_unix_timestamp(timestamp.timestamp())
                .unwrap_or(time::OffsetDateTime::now_utc());
            handler.add_historical_price("AAPL", offset_datetime, *price);
        }

        handler
    }

    #[test]
    fn test_breakout_signal_generator_creation() {
        let generator = BreakoutSignalGenerator::new();
        assert_eq!(generator.signal_type(), SignalType::Breakout);

        // Test with custom calculator
        let calculator = BreakoutCalculator::with_settings(0.01, 1.5, vec![10]);
        let custom_generator = BreakoutSignalGenerator::with_calculator(calculator);
        assert_eq!(custom_generator.signal_type(), SignalType::Breakout);
    }

    #[test]
    fn test_calculate_signal_implementation() {
        let generator = BreakoutSignalGenerator::new();
        let market_data = create_test_market_data();

        // This test should fail initially because calculate_signal returns todo!()
        let result = generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data);

        // Once implemented, this should succeed
        assert!(result.is_ok());

        // Should return Some(signal) for valid data
        let signal_opt = result.unwrap();
        assert!(signal_opt.is_some());

        let signal = signal_opt.unwrap();
        assert_eq!(signal.symbol, "AAPL");
        assert_eq!(signal.timeframe, TimeFrame::Days1);

        // Signal strength should be in Carver range after conversion
        let strength = generator.extract_signal_strength(&signal);
        assert!(strength >= -20.0 && strength <= 20.0);
    }

    #[test]
    fn test_calculate_multi_timeframe_implementation() {
        let generator = BreakoutSignalGenerator::new();
        let market_data = create_test_market_data();

        // This test should fail initially because calculate_multi_timeframe returns todo!()
        let result = generator.calculate_multi_timeframe("AAPL", &market_data);

        // Once implemented, this should succeed
        assert!(result.is_ok());

        let metrics_opt = result.unwrap();
        assert!(metrics_opt.is_some());

        let metrics = metrics_opt.unwrap();
        // BreakoutMetrics doesn't have symbol field, but should have timeframe signals
        assert!(!metrics.timeframe_signals.is_empty());

        // Should have signals for multiple timeframes
        assert!(!metrics.timeframe_signals.is_empty());

        // Composite signal should be in Carver range
        assert!(metrics.composite_signal >= -20.0 && metrics.composite_signal <= 20.0);

        // Consensus strength should be valid percentage
        assert!(metrics.consensus_strength >= 0.0 && metrics.consensus_strength <= 1.0);
    }

    #[test]
    fn test_extract_signal_strength() {
        let generator = BreakoutSignalGenerator::new();

        // Create test breakout signal with known strength
        let test_signal = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 15.5, // Strong positive breakout
            breakout_type: BreakoutType::UpBreakout,
            current_price: 110.0,
            breakout_level: 100.0,
            lookback_high: 105.0,
            lookback_low: 95.0,
            volatility_normalized: 0.8,
            percentile_rank: 0.95,
        };

        // This test should fail initially because extract_signal_strength returns todo!()
        let strength = generator.extract_signal_strength(&test_signal);

        // Once implemented, should return the correct signal strength
        assert!((strength - 15.5).abs() < 1e-10);

        // Should be in Carver range
        assert!(strength >= -20.0 && strength <= 20.0);
    }

    #[test]
    fn test_to_signal_core_conversion() {
        let generator = BreakoutSignalGenerator::new();

        // Create test breakout signal
        let test_signal = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 12.3,
            breakout_type: BreakoutType::UpBreakout,
            current_price: 110.0,
            breakout_level: 100.0,
            lookback_high: 105.0,
            lookback_low: 95.0,
            volatility_normalized: 0.75,
            percentile_rank: 0.88,
        };

        // This test should fail initially because to_signal_core returns todo!()
        let core = generator.to_signal_core(&test_signal);

        // Once implemented, should have correct conversion
        assert_eq!(core.symbol, "AAPL");
        assert_eq!(core.timeframe, TimeFrame::Days1);
        assert_eq!(core.signal_type, SignalType::Breakout);

        // Signal strength should match original
        assert!((core.signal_strength - 12.3).abs() < 1e-10);

        // Should be in Carver range
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);

        // Volatility adjustment should be applied
        assert!((core.volatility_adjusted - 0.75).abs() < 1e-10);

        // Percentile rank should be preserved
        assert!((core.percentile_rank - 0.88).abs() < 1e-10);

        // Quality should be determined based on signal characteristics
        match core.quality {
            SignalQuality::High | SignalQuality::Medium | SignalQuality::Low => {
                // Valid quality assessment
            }
            SignalQuality::Filtered => {
                panic!("Strong breakout signal should not be filtered");
            }
        }
    }

    #[test]
    fn test_signal_strength_carver_range_compliance() {
        let generator = BreakoutSignalGenerator::new();

        // Test extreme positive breakout
        let strong_positive = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            breakout_type: BreakoutType::UpBreakout,
            current_price: 150.0,
            breakout_level: 100.0,
            lookback_high: 105.0,
            lookback_low: 95.0,
            volatility_normalized: 1.0,
            percentile_rank: 0.99,
        };

        let core = generator.to_signal_core(&strong_positive);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength > 10.0); // Should be strongly positive

        // Test extreme negative breakout
        let strong_negative = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            breakout_type: BreakoutType::DownBreakout,
            current_price: 50.0,
            breakout_level: 100.0,
            lookback_high: 105.0,
            lookback_low: 95.0,
            volatility_normalized: 1.0,
            percentile_rank: 0.01,
        };

        let core = generator.to_signal_core(&strong_negative);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength < -10.0); // Should be strongly negative

        // Test no breakout
        let no_breakout = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0,
            breakout_type: BreakoutType::NoBreakout,
            current_price: 100.0,
            breakout_level: 100.0,
            lookback_high: 105.0,
            lookback_low: 95.0,
            volatility_normalized: 0.0,
            percentile_rank: 0.5,
        };

        let core = generator.to_signal_core(&no_breakout);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength.abs() < 2.0); // Should be near zero
    }

    #[test]
    fn test_unsupported_symbol_handling() {
        let generator = BreakoutSignalGenerator::new();
        let market_data = MarketDataHandler::new(); // Empty market data

        // Should handle unsupported symbol gracefully
        let result = generator.calculate_signal("UNSUPPORTED", TimeFrame::Days1, &market_data);
        assert!(result.is_ok());

        // Should return None for unsupported symbols
        let signal_opt = result.unwrap();
        assert!(signal_opt.is_none());
    }

    #[test]
    fn test_compatibility_with_existing_breakout_calculator() {
        let generator = BreakoutSignalGenerator::new();
        let market_data = create_test_market_data();

        // Test that our unified interface produces same results as original calculator
        let original_calculator = BreakoutCalculator::new();

        // Get original result
        let original_signal =
            original_calculator.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        // Get unified interface result
        let unified_result = generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data);

        // Both should succeed or both should fail
        assert_eq!(
            original_signal.is_some(),
            unified_result.as_ref().map_or(false, |r| r.is_some())
        );

        if let (Some(original), Ok(Some(unified))) = (original_signal, unified_result) {
            // Signal strengths should be equivalent
            let original_strength = original.signal_strength;
            let unified_strength = generator.extract_signal_strength(&unified);

            assert!((original_strength - unified_strength).abs() < 1e-6);

            // Other properties should match
            assert_eq!(original.symbol, unified.symbol);
            assert_eq!(original.timeframe, unified.timeframe);
            assert_eq!(original.breakout_type, unified.breakout_type);
        }
    }
}
