//! Momentum Signal Generator
//!
//! Implements the SignalGenerator trait for momentum-based trading signals.
//! Provides unified interface for momentum calculation with multi-timeframe analysis.

use crate::market_data::{
    EnhancedMomentumMetrics, MarketDataHandler, MultiTimeframeMomentum, TimeFrame,
};
use crate::signals::core::{SignalCore, SignalGenerator, SignalQuality, SignalType};
use crate::signals::utils::SignalUtils;
use anyhow::Result;

/// Momentum signal structure containing all momentum-related data
#[derive(Debug, Clone)]
pub struct MomentumSignal {
    pub symbol: String,
    pub timeframe: TimeFrame,
    pub simple_momentum: f64,
    pub signal_strength: f64,
    pub enhanced_metrics: Option<EnhancedMomentumMetrics>,
    pub multi_timeframe: Option<MultiTimeframeMomentum>,
}

/// Multi-timeframe momentum metrics for comprehensive analysis
#[derive(Debug, Clone)]
pub struct MomentumMetrics {
    pub symbol: String,
    pub composite_signal: f64,
    pub consensus_strength: f64,
    pub timeframe_signals: std::collections::HashMap<TimeFrame, MomentumSignal>,
    pub quality_score: f64,
    pub multi_timeframe: Option<MultiTimeframeMomentum>,
}

/// Momentum signal generator implementing unified SignalGenerator interface
#[derive(Debug, Clone)]
pub struct MomentumSignalGenerator {
    lookback_period: usize,
}

impl MomentumSignalGenerator {
    /// Create new momentum signal generator with default settings
    pub fn new() -> Self {
        Self {
            lookback_period: 20, // Default 20-day momentum
        }
    }

    /// Create momentum signal generator with custom lookback period
    pub fn with_lookback(lookback_period: usize) -> Self {
        Self { lookback_period }
    }
}

impl SignalGenerator for MomentumSignalGenerator {
    type Signal = MomentumSignal;
    type Metrics = MomentumMetrics;

    fn calculate_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Signal>> {
        // Calculate basic momentum using market data handler
        let simple_momentum = market_data.calculate_momentum(symbol, self.lookback_period);

        if let Some(momentum) = simple_momentum {
            // Get enhanced metrics if available
            let enhanced_metrics =
                market_data.calculate_enhanced_momentum(symbol, self.lookback_period);

            // Get multi-timeframe data if available
            let multi_timeframe = market_data.calculate_multi_timeframe_momentum(symbol);

            // Calculate signal strength using enhanced metrics or simple momentum
            let signal_strength = if let Some(ref enhanced) = enhanced_metrics {
                enhanced.risk_adjusted_momentum
            } else {
                momentum
            };

            Ok(Some(MomentumSignal {
                symbol: symbol.to_string(),
                timeframe,
                simple_momentum: momentum,
                signal_strength,
                enhanced_metrics,
                multi_timeframe,
            }))
        } else {
            Ok(None)
        }
    }

    fn calculate_multi_timeframe(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Metrics>> {
        // Get multi-timeframe momentum data
        let multi_timeframe = market_data.calculate_multi_timeframe_momentum(symbol);

        if let Some(mtf_data) = multi_timeframe {
            // Calculate signals for each timeframe
            let mut timeframe_signals = std::collections::HashMap::new();

            for (tf, enhanced_metrics) in &mtf_data.timeframe_metrics {
                let signal = MomentumSignal {
                    symbol: symbol.to_string(),
                    timeframe: *tf,
                    simple_momentum: enhanced_metrics.simple_momentum,
                    signal_strength: enhanced_metrics.risk_adjusted_momentum,
                    enhanced_metrics: Some(enhanced_metrics.clone()),
                    multi_timeframe: Some(mtf_data.clone()),
                };
                timeframe_signals.insert(*tf, signal);
            }

            // Calculate consensus strength
            let positive_signals = timeframe_signals
                .values()
                .filter(|s| s.simple_momentum > 0.0)
                .count() as f64;
            let total_signals = timeframe_signals.len() as f64;
            let consensus_strength = if total_signals > 0.0 {
                positive_signals.max(total_signals - positive_signals) / total_signals
            } else {
                0.0
            };

            // Use composite score from multi-timeframe data
            let composite_signal = mtf_data.composite_score;

            // Calculate quality score based on consensus and signal strength
            let quality_score = consensus_strength * composite_signal.abs().min(1.0);

            Ok(Some(MomentumMetrics {
                symbol: symbol.to_string(),
                composite_signal,
                consensus_strength,
                timeframe_signals,
                quality_score,
                multi_timeframe: Some(mtf_data),
            }))
        } else {
            Ok(None)
        }
    }

    fn extract_signal_strength(&self, signal: &Self::Signal) -> f64 {
        // Extract signal strength directly if available
        if signal.signal_strength != 0.0 {
            signal.signal_strength
        } else {
            // Calculate from simple momentum with Carver scaling
            let momentum_scaled = signal.simple_momentum * 20.0;
            SignalUtils::clamp_to_carver_range(momentum_scaled)
        }
    }

    fn to_signal_core(&self, signal: &Self::Signal) -> SignalCore {
        // Calculate Carver-style signal strength
        let carver_strength = self.calculate_carver_signal_strength(signal);

        // Assess signal quality
        let quality = self.assess_signal_quality(signal, carver_strength);

        // Calculate percentile rank from momentum (0.5 = neutral, >0.5 = positive momentum)
        let percentile_rank = if signal.simple_momentum > 0.0 {
            0.5 + (signal.simple_momentum.min(0.5) / 0.5) * 0.5
        } else {
            0.5 - ((-signal.simple_momentum).min(0.5) / 0.5) * 0.5
        }
        .clamp(0.0, 1.0);

        // Use volatility from enhanced metrics or default
        let volatility_adjusted = if let Some(ref enhanced) = signal.enhanced_metrics {
            enhanced.volatility.min(1.0)
        } else {
            0.25 // Default volatility estimate
        };

        SignalCore::new(
            signal.symbol.clone(),
            signal.timeframe,
            carver_strength,
            SignalType::Momentum,
            percentile_rank,
            volatility_adjusted,
            quality,
        )
    }

    fn signal_type(&self) -> SignalType {
        SignalType::Momentum
    }
}

/// Private implementation methods
impl MomentumSignalGenerator {
    /// Validate momentum signal data for consistency
    fn validate_signal(&self, signal: &MomentumSignal) -> bool {
        // Basic validation
        if signal.simple_momentum.is_nan() || signal.simple_momentum.is_infinite() {
            return false;
        }

        if signal.signal_strength.is_nan() || signal.signal_strength.is_infinite() {
            return false;
        }

        // Enhanced metrics validation
        if let Some(ref enhanced) = signal.enhanced_metrics {
            if enhanced.volatility <= 0.0 || enhanced.volatility.is_nan() {
                return false;
            }

            if enhanced.sharpe_ratio.is_nan() || enhanced.sharpe_ratio.is_infinite() {
                return false;
            }
        }

        true
    }

    /// Calculate position sizing hint based on momentum characteristics
    fn calculate_position_hint(&self, signal: &MomentumSignal, carver_strength: f64) -> f64 {
        let base_size = carver_strength.abs() / 20.0; // 0-1 scale

        // Adjust for momentum quality
        let quality_factor = if let Some(ref enhanced) = signal.enhanced_metrics {
            // High Sharpe ratio increases confidence
            let sharpe_factor = if enhanced.sharpe_ratio > 1.0 {
                1.2
            } else if enhanced.sharpe_ratio > 0.5 {
                1.0
            } else {
                0.8
            };

            // Moderate volatility adjustment
            let vol_factor = if enhanced.volatility > 0.4 {
                0.8 // Reduce size for high volatility
            } else if enhanced.volatility < 0.15 {
                0.9 // Slightly reduce for very low volatility
            } else {
                1.0
            };

            sharpe_factor * vol_factor
        } else {
            0.8 // Conservative without enhanced metrics
        };

        // Apply consensus boost
        let consensus_factor = if let Some(ref mtf) = signal.multi_timeframe {
            let positive_signals = mtf
                .timeframe_metrics
                .values()
                .filter(|m| m.risk_adjusted_momentum > 0.0)
                .count() as f64;
            let total_signals = mtf.timeframe_metrics.len() as f64;

            if total_signals > 0.0 {
                let consensus_ratio = positive_signals / total_signals;
                if consensus_ratio > 0.8 || consensus_ratio < 0.2 {
                    1.1 // Boost for strong consensus
                } else {
                    0.95 // Slight reduction for mixed signals
                }
            } else {
                1.0
            }
        } else {
            1.0
        };

        (base_size * quality_factor * consensus_factor).clamp(0.0, 1.0)
    }
    /// Calculate Carver-style signal strength from momentum metrics
    /// Implements sophisticated multi-factor signal strength calculation
    fn calculate_carver_signal_strength(&self, signal: &MomentumSignal) -> f64 {
        // Validate input signal first
        if !self.validate_signal(signal) {
            return 0.0;
        }
        // Start with signal strength or simple momentum
        let base_signal = if signal.signal_strength != 0.0 {
            signal.signal_strength
        } else {
            signal.simple_momentum
        };

        // Scale to Carver range (-20 to +20) only if not already scaled
        let scaled_signal = if signal.signal_strength != 0.0 && signal.signal_strength.abs() <= 1.0
        {
            // Signal strength appears to be in 0-1 range, scale to Carver range
            base_signal * 25.0 // Increased scaling for stronger signals
        } else if signal.signal_strength != 0.0 {
            // Signal strength already in reasonable range
            base_signal
        } else {
            // Simple momentum needs stronger scaling for significant values
            base_signal * 25.0 // 25% momentum -> 6.25 base, with multipliers -> >10
        };

        // Apply quality multipliers based on enhanced metrics
        let quality_multiplier = if let Some(ref enhanced) = signal.enhanced_metrics {
            let sharpe_multiplier = if enhanced.sharpe_ratio > 0.5 {
                1.2 // Boost high-quality signals
            } else if enhanced.sharpe_ratio < 0.1 {
                0.8 // Reduce low-quality signals
            } else {
                1.0
            };

            let vol_multiplier = if enhanced.volatility > 0.0 {
                (0.3 / enhanced.volatility).clamp(0.5, 1.5)
            } else {
                1.0
            };

            sharpe_multiplier * vol_multiplier
        } else {
            1.0
        };

        // Apply multi-timeframe consensus boost
        let consensus_multiplier = if let Some(ref mtf) = signal.multi_timeframe {
            let positive_signals = mtf
                .timeframe_metrics
                .values()
                .filter(|m| m.risk_adjusted_momentum > 0.0)
                .count() as f64;
            let total_signals = mtf.timeframe_metrics.len() as f64;

            if total_signals > 0.0 {
                let consensus_ratio = positive_signals / total_signals;
                if consensus_ratio > 0.75 {
                    1.3 // Strong consensus boost
                } else if consensus_ratio < 0.25 {
                    0.7 // Weak consensus penalty
                } else {
                    1.0
                }
            } else {
                1.0
            }
        } else {
            1.0
        };

        // Combine all factors with bounds checking
        let final_signal = scaled_signal * quality_multiplier * consensus_multiplier;

        // Apply momentum acceleration boost if available
        let acceleration_boost = if let Some(ref enhanced) = signal.enhanced_metrics {
            if enhanced.momentum_acceleration > 0.05 {
                1.1 // Boost for accelerating momentum
            } else if enhanced.momentum_acceleration < -0.05 {
                0.9 // Reduce for decelerating momentum
            } else {
                1.0
            }
        } else {
            1.0
        };

        let boosted_signal = final_signal * acceleration_boost;
        SignalUtils::clamp_to_carver_range(boosted_signal)
    }

    /// Assess momentum signal quality based on enhanced metrics
    fn assess_signal_quality(
        &self,
        signal: &MomentumSignal,
        carver_strength: f64,
    ) -> SignalQuality {
        let strength_abs = carver_strength.abs();

        // Base quality assessment from signal strength
        let base_quality = if strength_abs > 15.0 {
            SignalQuality::High
        } else if strength_abs > 5.0 {
            SignalQuality::Medium
        } else if strength_abs > 1.0 {
            SignalQuality::Low
        } else {
            SignalQuality::Filtered
        };

        // Enhance quality assessment with enhanced metrics
        if let Some(ref enhanced) = signal.enhanced_metrics {
            // High Sharpe ratio upgrades quality
            if enhanced.sharpe_ratio > 1.0 && base_quality == SignalQuality::Medium {
                return SignalQuality::High;
            }

            // Low Sharpe ratio downgrades quality
            if enhanced.sharpe_ratio < 0.2 {
                return match base_quality {
                    SignalQuality::High => SignalQuality::Medium,
                    SignalQuality::Medium => SignalQuality::Low,
                    SignalQuality::Low => SignalQuality::Filtered,
                    SignalQuality::Filtered => SignalQuality::Filtered,
                };
            }

            // Very high volatility downgrades quality
            if enhanced.volatility > 0.5 {
                return match base_quality {
                    SignalQuality::High => SignalQuality::Medium,
                    SignalQuality::Medium => SignalQuality::Low,
                    _ => base_quality,
                };
            }
        }

        base_quality
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::{MarketDataHandler, TimeFrame};

    /// Helper function to create test market data handler with momentum-suitable data
    fn create_test_market_data() -> MarketDataHandler {
        use chrono::{Duration, Utc};
        use time;

        let mut handler = MarketDataHandler::new();

        // Register the symbol first
        handler.register_symbol(1, "AAPL".to_string());

        // Add test price data with clear momentum pattern (uptrend)
        let test_prices = vec![
            100.0, 101.0, 102.5, 103.0, 104.5, 105.0, 106.5, 107.0, 108.5, 109.0, 110.5, 111.0,
            112.5, 113.0, 114.5, 115.0, 116.5, 117.0, 118.5, 119.0, 120.5, 121.0, 122.5, 123.0,
            124.5, 125.0, 126.5, 127.0, 128.5, 129.0, 130.5, 131.0, 132.5, 133.0, 134.5, 135.0,
            136.5, 137.0, 138.5, 139.0,
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
    fn test_momentum_signal_generator_creation() {
        let generator = MomentumSignalGenerator::new();
        assert_eq!(generator.signal_type(), SignalType::Momentum);
        assert_eq!(generator.lookback_period, 20);

        // Test with custom lookback
        let custom_generator = MomentumSignalGenerator::with_lookback(14);
        assert_eq!(custom_generator.lookback_period, 14);
        assert_eq!(custom_generator.signal_type(), SignalType::Momentum);
    }

    #[test]
    fn test_calculate_signal_implementation() {
        let generator = MomentumSignalGenerator::new();
        let market_data = create_test_market_data();

        // This test should fail initially because calculate_signal returns todo!()
        let result = generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data);

        // Once implemented, this should succeed
        assert!(result.is_ok());

        // Should return Some(signal) for valid uptrend data
        let signal_opt = result.unwrap();
        assert!(signal_opt.is_some());

        let signal = signal_opt.unwrap();
        assert_eq!(signal.symbol, "AAPL");
        assert_eq!(signal.timeframe, TimeFrame::Days1);

        // Should detect positive momentum in uptrending data
        assert!(signal.simple_momentum > 0.0);

        // Signal strength should be in Carver range after conversion
        let strength = generator.extract_signal_strength(&signal);
        assert!(strength >= -20.0 && strength <= 20.0);
        assert!(strength > 0.0); // Should be positive for uptrend
    }

    #[test]
    fn test_calculate_multi_timeframe_implementation() {
        let generator = MomentumSignalGenerator::new();
        let market_data = create_test_market_data();

        // This test should fail initially because calculate_multi_timeframe returns todo!()
        let result = generator.calculate_multi_timeframe("AAPL", &market_data);

        // Once implemented, this should succeed
        assert!(result.is_ok());

        let metrics_opt = result.unwrap();
        assert!(metrics_opt.is_some());

        let metrics = metrics_opt.unwrap();
        assert_eq!(metrics.symbol, "AAPL");

        // Should have multiple timeframe signals
        assert!(!metrics.timeframe_signals.is_empty());

        // Composite signal should be in Carver range
        assert!(metrics.composite_signal >= -20.0 && metrics.composite_signal <= 20.0);
        assert!(metrics.composite_signal > 0.0); // Should be positive for uptrend

        // Should have consensus strength
        assert!(metrics.consensus_strength >= 0.0 && metrics.consensus_strength <= 1.0);

        // Quality score should be meaningful
        assert!(metrics.quality_score >= 0.0);
    }

    #[test]
    fn test_extract_signal_strength() {
        let generator = MomentumSignalGenerator::new();

        // Create test momentum signal with known values
        let test_signal = MomentumSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            simple_momentum: 0.15, // 15% momentum
            signal_strength: 8.5,  // Pre-calculated Carver strength
            enhanced_metrics: None,
            multi_timeframe: None,
        };

        // This test should fail initially because extract_signal_strength returns todo!()
        let strength = generator.extract_signal_strength(&test_signal);

        // Once implemented, should return the signal strength
        assert!((strength - 8.5).abs() < 1e-10);

        // Should be in Carver range
        assert!(strength >= -20.0 && strength <= 20.0);
    }

    #[test]
    fn test_to_signal_core_conversion() {
        let generator = MomentumSignalGenerator::new();

        // Create test momentum signal with enhanced metrics
        let enhanced_metrics = EnhancedMomentumMetrics {
            simple_momentum: 0.15,
            risk_adjusted_momentum: 0.12,
            volatility_normalized_momentum: 0.18,
            momentum_acceleration: 0.05,
            volatility: 0.25,
            sharpe_ratio: 0.8,
            timeframe: TimeFrame::Days1,
        };

        let test_signal = MomentumSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            simple_momentum: 0.15,
            signal_strength: 0.0, // Will be calculated
            enhanced_metrics: Some(enhanced_metrics),
            multi_timeframe: None,
        };

        // This test should fail initially because to_signal_core returns todo!()
        let core = generator.to_signal_core(&test_signal);

        // Once implemented, should have correct conversion
        assert_eq!(core.symbol, "AAPL");
        assert_eq!(core.timeframe, TimeFrame::Days1);
        assert_eq!(core.signal_type, SignalType::Momentum);

        // Signal strength should be properly calculated and in Carver range
        assert!(core.signal_strength >= -20.0 && core.signal_strength <= 20.0);
        assert!(core.signal_strength > 0.0); // Should be positive for positive momentum

        // Should use enhanced metrics for quality assessment
        assert!(core.volatility_adjusted >= 0.0 && core.volatility_adjusted <= 1.0);
        assert!(core.percentile_rank >= 0.0 && core.percentile_rank <= 1.0);

        // Quality should be assessed based on enhanced metrics
        match core.quality {
            SignalQuality::High | SignalQuality::Medium | SignalQuality::Low => {
                // Valid quality assessment
            }
            SignalQuality::Filtered => {
                panic!("Strong momentum signal with good Sharpe ratio should not be filtered");
            }
        }
    }

    #[test]
    fn test_carver_signal_strength_calculation() {
        let generator = MomentumSignalGenerator::new();

        // Test strong momentum signal
        let strong_signal = MomentumSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            simple_momentum: 0.25, // Strong 25% momentum
            signal_strength: 0.0,  // Will be calculated
            enhanced_metrics: Some(EnhancedMomentumMetrics {
                simple_momentum: 0.25,
                risk_adjusted_momentum: 0.22,
                volatility_normalized_momentum: 0.28,
                momentum_acceleration: 0.08,
                volatility: 0.20,  // Moderate volatility
                sharpe_ratio: 1.2, // High Sharpe ratio
                timeframe: TimeFrame::Days1,
            }),
            multi_timeframe: None,
        };

        let core = generator.to_signal_core(&strong_signal);

        // Strong momentum with high Sharpe should produce strong signal
        assert!(core.signal_strength > 10.0);
        assert!(core.signal_strength <= 20.0);
        assert_eq!(core.quality, SignalQuality::High);

        // Test weak momentum signal
        let weak_signal = MomentumSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            simple_momentum: 0.03, // Weak 3% momentum
            signal_strength: 0.0,
            enhanced_metrics: Some(EnhancedMomentumMetrics {
                simple_momentum: 0.03,
                risk_adjusted_momentum: 0.025,
                volatility_normalized_momentum: 0.035,
                momentum_acceleration: 0.01,
                volatility: 0.35,  // High volatility
                sharpe_ratio: 0.2, // Low Sharpe ratio
                timeframe: TimeFrame::Days1,
            }),
            multi_timeframe: None,
        };

        let core = generator.to_signal_core(&weak_signal);

        // Weak momentum with poor metrics should produce weak signal
        assert!(core.signal_strength.abs() < 5.0);
        assert!(matches!(
            core.quality,
            SignalQuality::Low | SignalQuality::Filtered
        ));
    }

    #[test]
    fn test_multi_timeframe_consensus_boost() {
        let generator = MomentumSignalGenerator::new();
        let market_data = create_test_market_data();

        // Calculate multi-timeframe metrics
        let result = generator.calculate_multi_timeframe("AAPL", &market_data);
        assert!(result.is_ok());

        let metrics = result
            .unwrap()
            .expect("Should have momentum metrics for uptrending data");

        // With strong uptrend data, should have high consensus
        assert!(metrics.consensus_strength > 0.7); // Strong consensus expected

        // Composite signal should benefit from consensus boost
        assert!(metrics.composite_signal > 5.0); // Should be amplified by consensus

        // Individual timeframe signals should mostly agree
        let positive_signals = metrics
            .timeframe_signals
            .values()
            .filter(|s| s.simple_momentum > 0.0)
            .count();
        let total_signals = metrics.timeframe_signals.len();

        // Most timeframes should agree on positive momentum
        assert!(positive_signals as f64 / total_signals as f64 > 0.6);
    }

    #[test]
    fn test_enhanced_metrics_integration() {
        let generator = MomentumSignalGenerator::new();
        let market_data = create_test_market_data();

        // Calculate signal and verify enhanced metrics are used
        let result = generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data);
        assert!(result.is_ok());

        let signal = result.unwrap().expect("Should have signal for valid data");

        // Enhanced metrics should be calculated if available
        if let Some(ref enhanced) = signal.enhanced_metrics {
            // Risk-adjusted momentum should be reasonable
            assert!(enhanced.risk_adjusted_momentum > 0.0);
            assert!(enhanced.volatility > 0.0);

            // Should influence signal strength calculation
            let core = generator.to_signal_core(&signal);

            // High Sharpe ratio should boost signal quality
            if enhanced.sharpe_ratio > 0.5 {
                assert!(!matches!(core.quality, SignalQuality::Filtered));
            }
        }
    }

    #[test]
    fn test_unsupported_symbol_handling() {
        let generator = MomentumSignalGenerator::new();
        let market_data = MarketDataHandler::new(); // Empty market data

        // Should handle unsupported symbol gracefully
        let result = generator.calculate_signal("UNSUPPORTED", TimeFrame::Days1, &market_data);
        assert!(result.is_ok());

        // Should return None for unsupported symbols
        let signal_opt = result.unwrap();
        assert!(signal_opt.is_none());

        // Multi-timeframe should also handle gracefully
        let mtf_result = generator.calculate_multi_timeframe("UNSUPPORTED", &market_data);
        assert!(mtf_result.is_ok());
        assert!(mtf_result.unwrap().is_none());
    }

    #[test]
    fn test_compatibility_with_existing_momentum_calculation() {
        let generator = MomentumSignalGenerator::new();
        let market_data = create_test_market_data();

        // Test that unified interface produces consistent results with market_data methods
        let unified_result = generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data);
        let direct_momentum = market_data.calculate_momentum("AAPL", generator.lookback_period);

        // Both should succeed or both should fail
        assert_eq!(unified_result.is_ok(), direct_momentum.is_some());

        if let (Ok(Some(unified_signal)), Some(direct_mom)) = (unified_result, direct_momentum) {
            // Simple momentum values should be equivalent
            assert!((unified_signal.simple_momentum - direct_mom).abs() < 1e-6);
        }

        // Test enhanced metrics compatibility
        let enhanced_direct =
            market_data.calculate_enhanced_momentum("AAPL", generator.lookback_period);
        let _mtf_direct = market_data.calculate_multi_timeframe_momentum("AAPL");

        if let Ok(Some(unified_signal)) =
            generator.calculate_signal("AAPL", TimeFrame::Days1, &market_data)
        {
            // Enhanced metrics should match if both are available
            match (unified_signal.enhanced_metrics, enhanced_direct) {
                (Some(unified_enhanced), Some(direct_enhanced)) => {
                    assert!(
                        (unified_enhanced.risk_adjusted_momentum
                            - direct_enhanced.risk_adjusted_momentum)
                            .abs()
                            < 1e-6
                    );
                    assert!(
                        (unified_enhanced.sharpe_ratio - direct_enhanced.sharpe_ratio).abs() < 1e-6
                    );
                }
                (None, None) => {
                    // Both None is acceptable
                }
                _ => {
                    // Different availability might be acceptable depending on implementation
                }
            }
        }
    }
}
