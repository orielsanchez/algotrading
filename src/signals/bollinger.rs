//! Bollinger Signal Generator
//!
//! Implements the SignalGenerator trait for Bollinger band-based trading signals.
//! Provides unified interface for existing BollingerCalculator functionality.

use crate::bollinger::{
    BollingerCalculator, BollingerMetrics, BollingerSignal, BollingerSignalType, VolatilityRegime,
};
use crate::market_data::{MarketDataHandler, TimeFrame};
use crate::signals::core::{SignalCore, SignalGenerator, SignalQuality, SignalType};
use crate::signals::utils::SignalUtils;
use anyhow::Result;

/// Bollinger signal generator implementing unified SignalGenerator interface
#[derive(Debug, Clone)]
pub struct BollingerSignalGenerator {
    calculator: BollingerCalculator,
}

impl BollingerSignalGenerator {
    /// Create new bollinger signal generator with default settings
    pub fn new() -> Self {
        Self {
            calculator: BollingerCalculator::new(),
        }
    }

    /// Create bollinger signal generator with custom calculator
    pub fn with_calculator(calculator: BollingerCalculator) -> Self {
        Self { calculator }
    }
}

impl SignalGenerator for BollingerSignalGenerator {
    type Signal = BollingerSignal;
    type Metrics = BollingerMetrics;

    fn calculate_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Signal>> {
        // Delegate to existing BollingerCalculator
        Ok(self
            .calculator
            .calculate_bollinger_signal(symbol, timeframe, market_data))
    }

    fn calculate_multi_timeframe(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Metrics>> {
        // Delegate to existing BollingerCalculator multi-timeframe
        Ok(self
            .calculator
            .calculate_multi_timeframe_bollinger(symbol, market_data))
    }

    fn extract_signal_strength(&self, signal: &Self::Signal) -> f64 {
        // Extract signal strength from BollingerSignal
        signal.signal_strength
    }

    fn to_signal_core(&self, signal: &Self::Signal) -> SignalCore {
        // Validate signal before processing
        if !self.validate_signal(signal) {
            // Return a filtered signal for invalid data
            return SignalCore::new(
                signal.symbol.clone(),
                signal.timeframe,
                0.0,
                SignalType::MeanReversion,
                0.5,                             // Neutral percentile
                signal.bands.bandwidth.max(0.1), // Safe fallback
                SignalQuality::Filtered,
            );
        }
        // Convert BollingerSignal to SignalCore with enhanced Carver scaling
        let carver_strength = if signal.signal_strength == 0.0 {
            // Calculate signal strength from Bollinger bands position using improved algorithm
            let base_strength = match signal.signal_type {
                BollingerSignalType::MeanReversion => {
                    // Enhanced mean reversion using non-linear percent_b scaling
                    if signal.bands.percent_b < 0.2 {
                        // Oversold condition - positive signal (buy)
                        // Use exponential scaling for extreme positions
                        let oversold_intensity = (0.2 - signal.bands.percent_b) / 0.2; // 0-1 scale
                        let exponential_strength = oversold_intensity.powf(1.5) * 18.0;
                        exponential_strength.min(18.0)
                    } else if signal.bands.percent_b > 0.8 {
                        // Overbought condition - negative signal (sell)
                        let overbought_intensity = (signal.bands.percent_b - 0.8) / 0.2; // 0-1 scale
                        let exponential_strength = overbought_intensity.powf(1.5) * 18.0;
                        -exponential_strength.min(18.0)
                    } else {
                        // Neutral zone with smooth transition
                        let distance_from_middle = (signal.bands.percent_b - 0.5).abs();
                        if distance_from_middle < 0.1 {
                            0.0
                        } else {
                            // Smooth transition using sigmoid-like function
                            let normalized_distance = (distance_from_middle - 0.1) / 0.2; // 0-1 scale
                            let smooth_strength = normalized_distance.powf(2.0) * 6.0; // Gradual increase
                            if signal.bands.percent_b > 0.5 {
                                -smooth_strength
                            } else {
                                smooth_strength
                            }
                        }
                    }
                }
                BollingerSignalType::Breakout => {
                    // Enhanced breakout with volatility consideration
                    if signal.bands.percent_b > 1.0 {
                        // Price above upper band - bullish breakout
                        let breakout_distance = signal.bands.percent_b - 1.0;
                        // Scale by bandwidth (wider bands = stronger breakout)
                        let bandwidth_adjusted = breakout_distance * (1.0 + signal.bands.bandwidth);
                        let breakout_strength = bandwidth_adjusted * 25.0;
                        breakout_strength.min(18.0)
                    } else if signal.bands.percent_b < 0.0 {
                        // Price below lower band - bearish breakout
                        let breakout_distance = signal.bands.percent_b.abs();
                        let bandwidth_adjusted = breakout_distance * (1.0 + signal.bands.bandwidth);
                        let breakout_strength = bandwidth_adjusted * 25.0;
                        -breakout_strength.min(18.0)
                    } else {
                        // Within bands - potential early breakout signal
                        if signal.bands.percent_b > 0.9 || signal.bands.percent_b < 0.1 {
                            let edge_proximity = if signal.bands.percent_b > 0.9 {
                                (signal.bands.percent_b - 0.9) / 0.1
                            } else {
                                (0.1 - signal.bands.percent_b) / 0.1
                            };
                            let weak_breakout = edge_proximity * 4.0; // Weak early signal
                            if signal.bands.percent_b > 0.9 {
                                weak_breakout
                            } else {
                                -weak_breakout
                            }
                        } else {
                            0.0
                        }
                    }
                }
                BollingerSignalType::Squeeze => {
                    // Enhanced squeeze analysis with volatility timing
                    if signal.band_squeeze {
                        // Direction bias from percent_b with non-linear scaling
                        let direction_bias = if signal.bands.percent_b > 0.5 {
                            let upward_bias = (signal.bands.percent_b - 0.5) / 0.5; // 0-1 scale
                            upward_bias.powf(1.2) // Slight non-linearity
                        } else {
                            let downward_bias = (0.5 - signal.bands.percent_b) / 0.5; // 0-1 scale
                            -downward_bias.powf(1.2)
                        };

                        // Squeeze intensity (tighter bands = stronger signal)
                        let squeeze_intensity = if signal.bands.bandwidth < 0.05 {
                            1.0 // Maximum intensity
                        } else if signal.bands.bandwidth < 0.1 {
                            0.7 // Moderate intensity
                        } else {
                            0.4 // Weak intensity
                        };

                        direction_bias * squeeze_intensity * 12.0 // Moderate signal strength
                    } else {
                        0.0
                    }
                }
                BollingerSignalType::Neutral => 0.0,
            };

            // Enhanced volatility regime adjustment
            let volatility_multiplier = if signal.bands.bandwidth > 0.2 {
                1.1 // High volatility - slightly boost signals
            } else if signal.bands.bandwidth < 0.05 {
                0.9 // Low volatility - slightly dampen signals
            } else {
                1.0 // Normal volatility
            };

            // Band squeeze enhancement (more sophisticated than simple multiplier)
            let squeeze_enhancement = if signal.band_squeeze {
                match signal.signal_type {
                    BollingerSignalType::Squeeze => 1.0, // Already accounted for
                    BollingerSignalType::MeanReversion => 0.8, // Dampen mean reversion during squeeze
                    BollingerSignalType::Breakout => 1.3,      // Boost breakout during squeeze
                    BollingerSignalType::Neutral => 1.0,
                }
            } else {
                1.0
            };

            base_strength * volatility_multiplier * squeeze_enhancement
        } else {
            // Use existing signal strength with validation
            signal.signal_strength
        };

        // Clamp to Carver range
        let clamped_strength = SignalUtils::clamp_to_carver_range(carver_strength);

        // Assess signal quality
        let quality = self.assess_signal_quality(signal, clamped_strength);

        SignalCore::new(
            signal.symbol.clone(),
            signal.timeframe,
            clamped_strength,
            SignalType::MeanReversion,
            signal.bands.percent_b, // Use percent_b as percentile rank
            signal.bands.bandwidth, // Use bandwidth as volatility measure
            quality,
        )
    }

    fn signal_type(&self) -> SignalType {
        SignalType::MeanReversion
    }
}

/// Private implementation methods
impl BollingerSignalGenerator {
    /// Validate signal data for consistency and reasonable values
    fn validate_signal(&self, signal: &BollingerSignal) -> bool {
        // Basic data validation
        if signal.current_price <= 0.0 {
            return false;
        }

        // Bollinger bands validation
        if signal.bands.upper_band <= signal.bands.lower_band {
            return false;
        }

        if signal.bands.middle_line < signal.bands.lower_band
            || signal.bands.middle_line > signal.bands.upper_band
        {
            return false;
        }

        // Percent_b should be meaningful (though can be outside 0-1 for breakouts)
        if signal.bands.percent_b.is_nan() || signal.bands.percent_b.is_infinite() {
            return false;
        }

        // Bandwidth should be positive
        if signal.bands.bandwidth <= 0.0 || signal.bands.bandwidth.is_nan() {
            return false;
        }

        // Signal strength should be reasonable if set
        if signal.signal_strength.is_nan() || signal.signal_strength.is_infinite() {
            return false;
        }

        true
    }

    /// Calculate dynamic position sizing hint based on signal characteristics
    fn calculate_position_hint(&self, signal: &BollingerSignal, carver_strength: f64) -> f64 {
        let base_size = carver_strength.abs() / 20.0; // 0-1 scale

        // Adjust for volatility regime
        let volatility_factor = if signal.bands.bandwidth > 0.15 {
            0.8 // Reduce size in high volatility
        } else if signal.bands.bandwidth < 0.05 {
            0.6 // Reduce size in very low volatility (false signals)
        } else {
            1.0
        };

        // Adjust for signal type
        let signal_type_factor = match signal.signal_type {
            BollingerSignalType::MeanReversion => 1.0, // Full size for mean reversion
            BollingerSignalType::Breakout => 0.8,      // Slightly smaller for breakouts
            BollingerSignalType::Squeeze => 0.6,       // Smaller for squeeze (uncertain direction)
            BollingerSignalType::Neutral => 0.3,       // Very small for neutral
        };

        (base_size * volatility_factor * signal_type_factor).min(1.0)
    }
    /// Assess signal quality based on strength and bollinger characteristics
    /// Uses bandwidth, squeeze conditions, and signal strength for comprehensive assessment
    fn assess_signal_quality(
        &self,
        signal: &BollingerSignal,
        carver_strength: f64,
    ) -> SignalQuality {
        let strength_abs = carver_strength.abs();

        // Bandwidth-based quality assessment
        let bandwidth_factor = if signal.bands.bandwidth > 0.2 {
            1.2 // High volatility environment - boost quality
        } else if signal.bands.bandwidth < 0.05 {
            0.8 // Very tight bands - reduce quality
        } else {
            1.0 // Normal conditions
        };

        // Squeeze-based quality enhancement
        let squeeze_factor = if signal.band_squeeze {
            1.3 // Squeeze suggests upcoming volatility expansion
        } else {
            1.0
        };

        // Position within bands quality assessment
        let position_quality = match signal.signal_type {
            BollingerSignalType::MeanReversion => {
                // For mean reversion, extreme positions are higher quality
                if signal.bands.percent_b < 0.1 || signal.bands.percent_b > 0.9 {
                    1.3 // Extreme positions
                } else if signal.bands.percent_b < 0.3 || signal.bands.percent_b > 0.7 {
                    1.1 // Moderate positions
                } else {
                    0.9 // Neutral zone
                }
            }
            BollingerSignalType::Breakout => {
                // For breakouts, positions outside bands are highest quality
                if signal.bands.percent_b < 0.0 || signal.bands.percent_b > 1.0 {
                    1.4 // Outside bands
                } else {
                    0.8 // Inside bands
                }
            }
            BollingerSignalType::Squeeze => {
                // Squeeze quality depends on band tightness
                if signal.bands.bandwidth < 0.05 {
                    1.2 // Very tight squeeze
                } else {
                    1.0
                }
            }
            BollingerSignalType::Neutral => 0.5, // Neutral signals have lower quality
        };

        // Combined quality score
        let quality_score = strength_abs * bandwidth_factor * squeeze_factor * position_quality;

        // Map quality score to enum
        if quality_score > 20.0 {
            SignalQuality::High
        } else if quality_score > 8.0 {
            SignalQuality::Medium
        } else if quality_score > 2.0 {
            SignalQuality::Low
        } else {
            SignalQuality::Filtered
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bollinger::{BollingerCalculator, BollingerSignalType};
    use crate::market_data::{MarketDataHandler, TimeFrame};

    /// Helper function to create test market data handler
    fn create_test_market_data() -> MarketDataHandler {
        use chrono::{Duration, Utc};
        use time;

        let mut handler = MarketDataHandler::new();

        // Register the symbol first
        handler.register_symbol(1, "AAPL".to_string());

        // Add test price data for AAPL with some volatility
        let test_prices = vec![
            100.0, 102.0, 104.0, 103.0, 105.0, 107.0, 106.0, 108.0, 110.0, 109.0, 111.0, 113.0,
            112.0, 114.0, 116.0, 115.0, 117.0, 119.0, 118.0, 120.0, 122.0, 121.0, 123.0, 125.0,
            124.0, 126.0, 128.0, 127.0, 129.0, 130.0,
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
    fn test_bollinger_signal_generator_creation() {
        let generator = BollingerSignalGenerator::new();
        assert_eq!(generator.signal_type(), SignalType::MeanReversion);

        // Test with custom calculator
        let calculator = BollingerCalculator::with_settings(20, 2.0, 0.1);
        let custom_generator = BollingerSignalGenerator::with_calculator(calculator);
        assert_eq!(custom_generator.signal_type(), SignalType::MeanReversion);
    }

    #[test]
    fn test_calculate_signal_implementation() {
        let generator = BollingerSignalGenerator::new();
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
        let generator = BollingerSignalGenerator::new();
        let market_data = create_test_market_data();

        // This test should fail initially because calculate_multi_timeframe returns todo!()
        let result = generator.calculate_multi_timeframe("AAPL", &market_data);

        // Once implemented, this should succeed
        assert!(result.is_ok());

        let metrics_opt = result.unwrap();
        assert!(metrics_opt.is_some());

        let metrics = metrics_opt.unwrap();
        // BollingerMetrics should have timeframe signals
        assert!(!metrics.timeframe_signals.is_empty());

        // Composite signal should be in Carver range
        assert!(metrics.composite_signal >= -20.0 && metrics.composite_signal <= 20.0);

        // Should have volatility regime information
        assert!(matches!(
            metrics.volatility_regime,
            VolatilityRegime::Low | VolatilityRegime::Normal | VolatilityRegime::High
        ));
    }

    #[test]
    fn test_extract_signal_strength() {
        let generator = BollingerSignalGenerator::new();

        // Create test bollinger signal with known strength
        let test_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 12.5, // Strong mean reversion signal
            signal_type: BollingerSignalType::MeanReversion,
            current_price: 116.5,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 125.0,
                lower_band: 115.0,
                std_deviation: 2.5,
                bandwidth: 0.08,
                percent_b: 0.15, // Near lower band
            },
            band_squeeze: false,
        };

        // This test should fail initially because extract_signal_strength returns todo!()
        let strength = generator.extract_signal_strength(&test_signal);

        // Once implemented, should return the correct signal strength
        assert!((strength - 12.5).abs() < 1e-10);

        // Should be in Carver range
        assert!(strength >= -20.0 && strength <= 20.0);
    }

    #[test]
    fn test_to_signal_core_conversion() {
        let generator = BollingerSignalGenerator::new();

        // Create test bollinger signal
        let test_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: -8.7, // Mean reversion signal
            signal_type: BollingerSignalType::MeanReversion,
            current_price: 117.0,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 125.0,
                lower_band: 115.0,
                std_deviation: 2.5,
                bandwidth: 0.08,
                percent_b: 0.2, // Near lower band - expect reversion up
            },
            band_squeeze: false,
        };

        // This test should fail initially because to_signal_core returns todo!()
        let core = generator.to_signal_core(&test_signal);

        // Once implemented, should have correct conversion
        assert_eq!(core.symbol, "AAPL");
        assert_eq!(core.timeframe, TimeFrame::Days1);
        assert_eq!(core.signal_type, SignalType::MeanReversion);

        // Signal strength should match original
        assert!((core.signal_strength - (-8.7)).abs() < 1e-10);

        // Should be in Carver range
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);

        // Volatility adjustment should be applied
        assert!(core.volatility_adjusted >= 0.0 && core.volatility_adjusted <= 1.0);

        // Percentile rank should be calculated
        assert!(core.percentile_rank >= 0.0 && core.percentile_rank <= 1.0);

        // Quality should be determined based on signal characteristics
        match core.quality {
            SignalQuality::High | SignalQuality::Medium | SignalQuality::Low => {
                // Valid quality assessment
            }
            SignalQuality::Filtered => {
                panic!("Strong bollinger signal should not be filtered");
            }
        }
    }

    #[test]
    fn test_signal_strength_carver_range_compliance() {
        let generator = BollingerSignalGenerator::new();

        // Test extreme mean reversion signal (oversold)
        let strong_oversold = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            signal_type: BollingerSignalType::MeanReversion,
            current_price: 111.0,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 130.0,
                lower_band: 110.0,
                std_deviation: 5.0,
                bandwidth: 0.16,
                percent_b: 0.05, // Very close to lower band
            },
            band_squeeze: false,
        };

        let core = generator.to_signal_core(&strong_oversold);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength > 10.0); // Should be strongly positive (buy signal)

        // Test extreme overbought signal
        let strong_overbought = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            signal_type: BollingerSignalType::MeanReversion,
            current_price: 129.0,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 130.0,
                lower_band: 110.0,
                std_deviation: 5.0,
                bandwidth: 0.16,
                percent_b: 0.95, // Very close to upper band
            },
            band_squeeze: false,
        };

        let core = generator.to_signal_core(&strong_overbought);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength < -10.0); // Should be strongly negative (sell signal)

        // Test neutral signal (near middle band)
        let neutral_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0,
            signal_type: BollingerSignalType::Neutral,
            current_price: 121.0,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 130.0,
                lower_band: 110.0,
                std_deviation: 5.0,
                bandwidth: 0.16,
                percent_b: 0.55, // Near middle band
            },
            band_squeeze: false,
        };

        let core = generator.to_signal_core(&neutral_signal);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength.abs() < 3.0); // Should be near zero
    }

    #[test]
    fn test_breakout_signal_handling() {
        let generator = BollingerSignalGenerator::new();

        // Test bollinger breakout signal (price breaking out of bands)
        let breakout_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            signal_type: BollingerSignalType::Breakout,
            current_price: 130.0,
            bands: crate::bollinger::BollingerBands {
                middle_line: 120.0,
                upper_band: 125.0,
                lower_band: 115.0,
                std_deviation: 2.5,
                bandwidth: 0.08,
                percent_b: 1.5, // Price above upper band
            },
            band_squeeze: true, // Squeeze detected - volatility expansion
        };

        let core = generator.to_signal_core(&breakout_signal);
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);

        // Breakout signals should be positive (continuation)
        assert!(core.signal_strength > 5.0);

        // Quality should reflect volatility conditions
        assert!(!matches!(core.quality, SignalQuality::Filtered));
    }

    #[test]
    fn test_unsupported_symbol_handling() {
        let generator = BollingerSignalGenerator::new();
        let market_data = MarketDataHandler::new(); // Empty market data

        // Should handle unsupported symbol gracefully
        let result = generator.calculate_signal("UNSUPPORTED", TimeFrame::Days1, &market_data);
        assert!(result.is_ok());

        // Should return None for unsupported symbols
        let signal_opt = result.unwrap();
        assert!(signal_opt.is_none());
    }

    #[test]
    fn test_compatibility_with_existing_bollinger_calculator() {
        let generator = BollingerSignalGenerator::new();
        let market_data = create_test_market_data();

        // Test that our unified interface produces same results as original calculator
        let original_calculator = BollingerCalculator::new();

        // Get original result
        let original_signal =
            original_calculator.calculate_bollinger_signal("AAPL", TimeFrame::Days1, &market_data);

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
            assert_eq!(original.signal_type, unified.signal_type);
        }
    }
}
