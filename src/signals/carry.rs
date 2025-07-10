//! Carry Signal Generator
//!
//! Implements carry-based signals for forex pairs using interest rate differentials.
//! Follows Carver's systematic trading framework with unified signal interface.

use super::core::{SignalCore, SignalGenerator, SignalQuality, SignalType};
use super::utils::SignalUtils;
use crate::carry::{CarrySignal, CarrySignalType, InterestRateDifferential, MultiTimeframeCarry};
use crate::market_data::{MarketDataHandler, TimeFrame};
use anyhow::Result;
use std::collections::HashMap;

/// Unified carry signal generator implementing SignalGenerator trait
pub struct CarrySignalGenerator {
    lookback_periods: usize,
    volatility_threshold: f64,
    min_differential_threshold: f64,
    signal_scaling_factor: f64,
}

impl CarrySignalGenerator {
    /// Create new carry signal generator
    pub fn new(
        lookback_periods: usize,
        volatility_threshold: f64,
        min_differential_threshold: f64,
    ) -> Self {
        Self {
            lookback_periods,
            volatility_threshold,
            min_differential_threshold,
            signal_scaling_factor: 20.0, // Carver's -20 to +20 range
        }
    }

    /// Default carry signal generator for forex
    pub fn default_forex() -> Self {
        Self::new(252, 0.3, 0.01) // 1 year lookback, 30% volatility threshold, 1% min differential
    }

    /// Calculate carry signal using existing CarrySignal logic
    fn calculate_raw_carry_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
        interest_rates: &InterestRateDifferential,
    ) -> Result<CarrySignal> {
        // Get price history for volatility calculation
        let price_history = market_data
            .get_price_history(symbol)
            .ok_or_else(|| anyhow::anyhow!("No price history for symbol: {}", symbol))?;

        // Calculate volatility using shared utility
        let volatility = if price_history.prices.len() > 1 {
            let prices: Vec<f64> = price_history
                .prices
                .iter()
                .map(|(_, price)| *price)
                .collect();
            SignalUtils::calculate_volatility(&prices, true)? // Annualized
        } else {
            0.15 // Default 15% volatility
        };

        // Calculate carry yield annualized
        let carry_yield_annualized = interest_rates.differential / 100.0; // Convert from percentage

        // Calculate volatility-adjusted carry (Sharpe-like ratio)
        let volatility_adjusted_carry = if volatility > 0.0 {
            carry_yield_annualized / volatility
        } else {
            carry_yield_annualized * 10.0 // High signal if no volatility
        };

        // Calculate signal strength using Carver scaling
        let base_signal = volatility_adjusted_carry * self.signal_scaling_factor;

        // Apply regime adjustment based on volatility
        let average_volatility = 0.2; // Assume 20% average volatility for forex
        let regime_adjustment =
            SignalUtils::calculate_regime_adjustment(volatility, average_volatility);

        // Calculate quality multiplier
        let percentile_rank = 0.5; // Simplified - would need historical data for real calculation
        let quality_multiplier =
            SignalUtils::calculate_quality_multiplier(base_signal, volatility, percentile_rank);

        // Final signal strength
        let signal_strength = SignalUtils::clamp_to_carver_range(
            base_signal * regime_adjustment * quality_multiplier,
        );

        // Determine signal type
        let signal_type = if interest_rates.differential > self.min_differential_threshold {
            CarrySignalType::PositiveCarry
        } else if interest_rates.differential < -self.min_differential_threshold {
            CarrySignalType::NegativeCarry
        } else {
            CarrySignalType::Neutral
        };

        Ok(CarrySignal {
            symbol: symbol.to_string(),
            timeframe,
            signal_strength,
            signal_type,
            interest_differential: interest_rates.differential,
            carry_yield_annualized,
            volatility_adjusted_carry,
            percentile_rank,
            regime_adjustment,
        })
    }

    /// Get mock interest rate differential for testing
    /// In production, this would come from a real data provider
    fn get_interest_rates(&self, symbol: &str) -> Result<InterestRateDifferential> {
        use chrono::Utc;

        // Mock interest rate data for common forex pairs
        let (base_currency, quote_currency, base_rate, quote_rate) = match symbol {
            "USD/JPY" => ("USD", "JPY", 5.25, 0.1), // USD higher than JPY
            "AUD/USD" => ("AUD", "USD", 4.35, 5.25), // USD higher than AUD
            "EUR/GBP" => ("EUR", "GBP", 3.75, 5.25), // GBP higher than EUR
            "GBP/USD" => ("GBP", "USD", 5.25, 5.25), // Equal rates
            _ => {
                return Err(anyhow::anyhow!(
                    "No interest rate data for symbol: {}",
                    symbol
                ));
            }
        };

        Ok(InterestRateDifferential {
            base_currency: base_currency.to_string(),
            quote_currency: quote_currency.to_string(),
            base_rate,
            quote_rate,
            differential: base_rate - quote_rate,
            last_updated: Utc::now(),
        })
    }
}

impl SignalGenerator for CarrySignalGenerator {
    type Signal = CarrySignal;
    type Metrics = MultiTimeframeCarry;

    fn calculate_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Signal>> {
        // Get interest rate data
        let interest_rates = match self.get_interest_rates(symbol) {
            Ok(rates) => rates,
            Err(_) => return Ok(None), // Not a forex pair or no data available
        };

        // Calculate the carry signal
        match self.calculate_raw_carry_signal(symbol, timeframe, market_data, &interest_rates) {
            Ok(signal) => Ok(Some(signal)),
            Err(_) => Ok(None), // Failed to calculate signal
        }
    }

    fn calculate_multi_timeframe(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Metrics>> {
        // Calculate signals for different timeframes
        let timeframes = self.supported_timeframes();
        let mut timeframe_signals = HashMap::new();
        let mut signal_strengths = Vec::new();

        for timeframe in &timeframes[0..3] {
            // Use first 3 timeframes for carry
            if let Some(signal) = self.calculate_signal(symbol, *timeframe, market_data)? {
                signal_strengths.push(signal.signal_strength);
                timeframe_signals.insert(*timeframe, signal);
            }
        }

        if timeframe_signals.is_empty() {
            return Ok(None);
        }

        // Calculate consensus metrics
        let consensus_strength = SignalUtils::calculate_consensus_strength(&signal_strengths);
        let consensus_boost = if consensus_strength > 0.67 { 1.25 } else { 1.0 };

        // Calculate composite signal (not used in MultiTimeframeCarry but calculated for completeness)
        let _composite_signal = SignalUtils::calculate_composite_signal(
            &signal_strengths
                .iter()
                .enumerate()
                .map(|(i, &strength)| (timeframes[i], strength))
                .collect(),
            None,
        );

        // Extract individual signals for MultiTimeframeCarry structure
        let short_term = timeframe_signals
            .get(&timeframes[0])
            .cloned()
            .unwrap_or_else(|| self.create_default_signal(symbol, timeframes[0]));
        let medium_term = timeframe_signals
            .get(&timeframes[1])
            .cloned()
            .unwrap_or_else(|| self.create_default_signal(symbol, timeframes[1]));
        let long_term = timeframe_signals
            .get(&timeframes[2])
            .cloned()
            .unwrap_or_else(|| self.create_default_signal(symbol, timeframes[2]));

        Ok(Some(MultiTimeframeCarry {
            short_term,
            medium_term,
            long_term,
            consensus_score: consensus_strength,
            consensus_boost,
        }))
    }

    fn extract_signal_strength(&self, signal: &Self::Signal) -> f64 {
        signal.signal_strength
    }

    fn to_signal_core(&self, signal: &Self::Signal) -> SignalCore {
        let quality = if signal.signal_strength.abs() > 10.0 {
            SignalQuality::High
        } else if signal.signal_strength.abs() > 5.0 {
            SignalQuality::Medium
        } else if signal.signal_strength.abs() > 1.0 {
            SignalQuality::Low
        } else {
            SignalQuality::Filtered
        };

        SignalCore::new(
            signal.symbol.clone(),
            signal.timeframe,
            signal.signal_strength,
            SignalType::Carry,
            signal.percentile_rank,
            signal.volatility_adjusted_carry,
            quality,
        )
    }

    fn signal_type(&self) -> SignalType {
        SignalType::Carry
    }

    fn supported_timeframes(&self) -> Vec<TimeFrame> {
        vec![
            TimeFrame::Days2_8,  // Short-term carry
            TimeFrame::Days4_16, // Medium-term carry
            TimeFrame::Days8_32, // Long-term carry
        ]
    }
}

impl CarrySignalGenerator {
    /// Create a default signal for missing timeframes
    fn create_default_signal(&self, symbol: &str, timeframe: TimeFrame) -> CarrySignal {
        CarrySignal {
            symbol: symbol.to_string(),
            timeframe,
            signal_strength: 0.0,
            signal_type: CarrySignalType::Neutral,
            interest_differential: 0.0,
            carry_yield_annualized: 0.0,
            volatility_adjusted_carry: 0.0,
            percentile_rank: 0.5,
            regime_adjustment: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use time;

    fn create_test_market_data() -> MarketDataHandler {
        let mut handler = MarketDataHandler::new();

        // Add test price data for USD/JPY
        let mut prices = Vec::new();
        let base_time = Utc::now();
        for i in 0..10 {
            let time = base_time - chrono::Duration::days(10 - i);
            let price = 150.0 + (i as f64 * 0.5); // Trending upward
            prices.push((time, price));
        }

        add_price_series(&mut handler, "USD/JPY", prices);
        handler
    }

    fn add_price_series(
        handler: &mut MarketDataHandler,
        symbol: &str,
        prices: Vec<(DateTime<Utc>, f64)>,
    ) {
        handler.register_symbol(1, symbol.to_string());
        for (timestamp, price) in prices {
            let offset_datetime = time::OffsetDateTime::from_unix_timestamp(timestamp.timestamp())
                .unwrap_or(time::OffsetDateTime::now_utc());
            handler.add_historical_price(symbol, offset_datetime, price);
        }
    }

    #[test]
    fn test_carry_signal_generator_creation() {
        let generator = CarrySignalGenerator::new(252, 0.3, 0.01);
        assert_eq!(generator.lookback_periods, 252);
        assert_eq!(generator.volatility_threshold, 0.3);
        assert_eq!(generator.min_differential_threshold, 0.01);
    }

    #[test]
    fn test_carry_signal_calculation() {
        let generator = CarrySignalGenerator::default_forex();
        let market_data = create_test_market_data();

        let signal = generator
            .calculate_signal("USD/JPY", TimeFrame::Days4_16, &market_data)
            .unwrap();

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.symbol, "USD/JPY");
        assert_eq!(signal.timeframe, TimeFrame::Days4_16);
        assert!(signal.signal_strength >= -20.0 && signal.signal_strength <= 20.0);
    }

    #[test]
    fn test_multi_timeframe_carry() {
        let generator = CarrySignalGenerator::default_forex();
        let market_data = create_test_market_data();

        let metrics = generator
            .calculate_multi_timeframe("USD/JPY", &market_data)
            .unwrap();

        assert!(metrics.is_some());
        let metrics = metrics.unwrap();
        assert!(metrics.consensus_score >= 0.0 && metrics.consensus_score <= 1.0);
        assert!(metrics.consensus_boost >= 1.0);
    }

    #[test]
    fn test_signal_core_conversion() {
        let generator = CarrySignalGenerator::default_forex();
        let market_data = create_test_market_data();

        let signal = generator
            .calculate_signal("USD/JPY", TimeFrame::Days4_16, &market_data)
            .unwrap()
            .unwrap();

        let core = generator.to_signal_core(&signal);
        assert_eq!(core.signal_type, SignalType::Carry);
        assert_eq!(core.symbol, "USD/JPY");
        assert_eq!(core.signal_strength, signal.signal_strength);
    }

    #[test]
    fn test_unsupported_symbol() {
        let generator = CarrySignalGenerator::default_forex();
        let market_data = create_test_market_data();

        let signal = generator
            .calculate_signal(
                "AAPL", // Not a forex pair
                TimeFrame::Days4_16,
                &market_data,
            )
            .unwrap();

        assert!(signal.is_none());
    }
}
