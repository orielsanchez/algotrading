//! Example usage of the unified signal architecture
//!
//! This module demonstrates how to use the new signal generation framework
//! with the SignalCoordinator for unified signal combination.

use super::{CarrySignalGenerator, CoordinatorBuilder, SignalCore, SignalGenerator, SignalWeights};
use crate::market_data::MarketDataHandler;
use anyhow::Result;

/// Example of using the unified signal architecture
pub struct SignalExample;

impl SignalExample {
    /// Demonstrate basic signal generation and coordination
    pub fn run_basic_example(market_data: &MarketDataHandler) -> Result<()> {
        // Create signal generators
        let carry_generator = CarrySignalGenerator::default_forex();

        // Create signal coordinator with custom weights
        let custom_weights = SignalWeights {
            momentum: 0.4,
            breakout: 0.3,
            carry: 0.25,
            mean_reversion: 0.05,
        };

        let coordinator = CoordinatorBuilder::new()
            .with_weights(custom_weights)
            .with_consensus_threshold(0.75)
            .with_quality_threshold(2.0)
            .build()?;

        // Generate signals for a forex pair
        let symbol = "USD/JPY";

        // Calculate carry signal
        let carry_metrics = carry_generator.calculate_multi_timeframe(symbol, market_data)?;

        // Convert to signal cores for coordination
        let carry_core = if let Some(metrics) = carry_metrics {
            let signal = &metrics.short_term; // Use short-term signal as representative
            Some(carry_generator.to_signal_core(signal))
        } else {
            None
        };

        // In a real implementation, you would also have momentum and breakout signals
        let momentum_core: Option<SignalCore> = None; // Would come from momentum generator
        let breakout_core: Option<SignalCore> = None; // Would come from breakout generator
        let mean_reversion_core: Option<SignalCore> = None; // Would come from bollinger generator

        // Combine all signals
        let combined_signals = coordinator.combine_signals(
            momentum_core,
            breakout_core,
            carry_core,
            mean_reversion_core,
        );

        // Analyze the results
        println!("Signal Analysis for {}:", symbol);
        println!(
            "  Composite Strength: {:.2}",
            combined_signals.composite_strength
        );
        println!("  Agreement Score: {:.2}", combined_signals.agreement_score);

        if let Some(dominant) = &combined_signals.dominant_signal {
            println!("  Dominant Signal: {:?}", dominant);
        }

        if combined_signals.has_actionable_signals() {
            println!("  Status: ACTIONABLE");
            println!(
                "  Max Signal Strength: {:.2}",
                combined_signals.max_signal_strength()
            );
        } else {
            println!("  Status: FILTERED - No actionable signals");
        }

        // Print individual signal details
        if let Some(carry) = &combined_signals.carry {
            println!(
                "  Carry Signal: {:.2} ({})",
                carry.signal_strength,
                format!("{:?}", carry.quality)
            );
        }

        Ok(())
    }

    /// Demonstrate signal type filtering and analysis
    pub fn analyze_signal_types(market_data: &MarketDataHandler, symbols: &[&str]) -> Result<()> {
        let carry_generator = CarrySignalGenerator::default_forex();

        println!("Signal Type Analysis:");
        println!("==================");

        for symbol in symbols {
            println!("\n{}: ", symbol);

            // Try to generate carry signal
            match carry_generator.calculate_signal(
                symbol,
                crate::market_data::TimeFrame::Days4_16,
                market_data,
            )? {
                Some(signal) => {
                    let core = carry_generator.to_signal_core(&signal);
                    println!(
                        "  Carry Signal: {:.2} ({:?})",
                        core.signal_strength, core.quality
                    );
                    println!("  Type: {:?}", core.signal_type);
                    println!("  Actionable: {}", core.is_actionable());
                    println!(
                        "  Direction: {}",
                        match core.direction() {
                            1 => "LONG",
                            -1 => "SHORT",
                            0 => "NEUTRAL",
                            _ => "UNKNOWN",
                        }
                    );
                }
                None => {
                    println!("  No carry signal available (not a forex pair or insufficient data)");
                }
            }
        }

        Ok(())
    }

    /// Demonstrate advanced coordinator configuration
    pub fn advanced_coordination_example() -> Result<()> {
        // Create highly selective coordinator
        let selective_coordinator = CoordinatorBuilder::new()
            .with_weights(SignalWeights {
                momentum: 0.6,        // Emphasize trend following
                breakout: 0.25,       // Secondary confirmation
                carry: 0.1,           // Minor fundamental factor
                mean_reversion: 0.05, // Counter-trend filter
            })
            .with_consensus_threshold(0.8) // Require strong agreement
            .with_quality_threshold(5.0) // Only high-quality signals
            .with_cross_validation(true) // Enable signal validation
            .build()?;

        println!("Advanced Coordinator Configuration:");
        println!(
            "  Consensus Threshold: {:.1}%",
            selective_coordinator.config().consensus_threshold * 100.0
        );
        println!(
            "  Quality Threshold: {:.1}",
            selective_coordinator.config().quality_filter_threshold
        );
        println!(
            "  Cross Validation: {}",
            selective_coordinator.config().enable_cross_validation
        );

        // Create aggressive coordinator
        let aggressive_coordinator = CoordinatorBuilder::new()
            .with_weights(SignalWeights {
                momentum: 0.4,
                breakout: 0.4, // Equal weight trend signals
                carry: 0.15,
                mean_reversion: 0.05,
            })
            .with_consensus_threshold(0.5) // Lower consensus requirement
            .with_quality_threshold(1.0) // Accept weaker signals
            .build()?;

        println!("\nAggressive Coordinator Configuration:");
        println!("  Focus: Higher signal capture, lower selectivity");
        println!(
            "  Consensus Threshold: {:.1}%",
            aggressive_coordinator.config().consensus_threshold * 100.0
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::MarketDataHandler;

    #[test]
    fn test_signal_example_creation() {
        // Test that examples can be created without panicking
        let market_data = MarketDataHandler::new();

        // This should not panic even with empty market data
        let result = SignalExample::analyze_signal_types(&market_data, &["INVALID"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_advanced_coordination_config() {
        let result = SignalExample::advanced_coordination_example();
        assert!(result.is_ok());
    }
}
