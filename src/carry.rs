use crate::market_data::{MarketDataHandler, TimeFrame};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Carry signal types following Carver's framework
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CarrySignalType {
    PositiveCarry, // Higher yielding currency favored
    NegativeCarry, // Lower yielding currency favored
    Neutral,       // Minimal carry advantage
}

/// Interest rate differential data for carry calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterestRateDifferential {
    pub base_currency: String,
    pub quote_currency: String,
    pub base_rate: f64,    // Interest rate for base currency (annualized)
    pub quote_rate: f64,   // Interest rate for quote currency (annualized)
    pub differential: f64, // base_rate - quote_rate
    pub last_updated: DateTime<Utc>,
}

/// Carry signal with Carver-style strength scaling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrySignal {
    pub symbol: String,
    pub timeframe: TimeFrame,
    pub signal_strength: f64, // Carver -20 to +20 range
    pub signal_type: CarrySignalType,
    pub interest_differential: f64, // Annual interest rate differential (%)
    pub carry_yield_annualized: f64, // Expected annual carry yield
    pub volatility_adjusted_carry: f64, // Carry/volatility ratio
    pub percentile_rank: f64,       // Percentile rank vs historical differentials
    pub regime_adjustment: f64,     // Adjustment based on volatility regime
}

/// Multi-timeframe carry analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTimeframeCarry {
    pub short_term: CarrySignal,  // Daily/weekly carry
    pub medium_term: CarrySignal, // Monthly carry trend
    pub long_term: CarrySignal,   // Quarterly carry trend
    pub consensus_score: f64,     // Agreement across timeframes
    pub consensus_boost: f64,     // Carver-style consensus multiplier
}

/// Carry strategy calculator
pub struct CarryStrategy {
    lookback_periods: usize,
    volatility_threshold: f64,
    min_differential_threshold: f64, // Minimum rate differential for signal
    signal_scaling_factor: f64,      // Scaling for Carver -20/+20 range
}

impl CarryStrategy {
    /// Create new carry strategy with Carver parameters
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

    /// Calculate carry signal for a forex pair
    pub fn calculate_carry_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
        interest_rates: &InterestRateDifferential,
    ) -> Result<CarrySignal> {
        // Get price history to calculate volatility
        let price_history = market_data
            .get_price_history(symbol)
            .ok_or_else(|| anyhow::anyhow!("No price history for symbol: {}", symbol))?;

        // Calculate annualized volatility (simplified)
        let volatility = if price_history.prices.len() > 1 {
            let returns: Vec<f64> = price_history
                .prices
                .windows(2)
                .map(|w| (w[1].1 / w[0].1).ln())
                .collect();

            let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance = returns
                .iter()
                .map(|r| (r - mean_return).powi(2))
                .sum::<f64>()
                / returns.len() as f64;

            variance.sqrt() * (252.0_f64).sqrt() // Annualized volatility
        } else {
            0.15 // Default 15% volatility
        };

        // Calculate carry yield annualized
        let carry_yield_annualized = interest_rates.differential / 100.0;

        // Get percentile rank
        let percentile_rank =
            self.calculate_carry_percentiles(symbol, market_data, interest_rates.differential)?;

        // Calculate volatility-adjusted carry
        let volatility_adjusted_carry = if volatility > 0.0 {
            carry_yield_annualized / volatility
        } else {
            0.0
        };

        // Determine signal type
        let signal_type = self.determine_signal_type(interest_rates.differential, volatility);

        // Calculate signal strength
        let signal_strength =
            self.carry_to_signal_strength(carry_yield_annualized, volatility, percentile_rank);

        // Apply regime adjustment (simplified)
        let regime_adjustment = if volatility > 0.20 {
            0.3 // Extremely high volatility regime severely dampens signal
        } else if volatility > 0.15 {
            0.5 // Very high volatility regime severely dampens signal
        } else if volatility > 0.10 {
            0.7 // High volatility regime dampens signal
        } else {
            1.0
        };

        let adjusted_signal_strength = signal_strength * regime_adjustment;

        Ok(CarrySignal {
            symbol: symbol.to_string(),
            timeframe,
            signal_strength: adjusted_signal_strength,
            signal_type,
            interest_differential: interest_rates.differential,
            carry_yield_annualized,
            volatility_adjusted_carry,
            percentile_rank,
            regime_adjustment,
        })
    }

    /// Calculate multi-timeframe carry analysis
    pub fn calculate_multi_timeframe_carry(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
        interest_rates: &InterestRateDifferential,
    ) -> Result<MultiTimeframeCarry> {
        // Calculate carry signals for different timeframes
        let short_term =
            self.calculate_carry_signal(symbol, TimeFrame::Days2_8, market_data, interest_rates)?;
        let medium_term =
            self.calculate_carry_signal(symbol, TimeFrame::Days8_32, market_data, interest_rates)?;
        let long_term =
            self.calculate_carry_signal(symbol, TimeFrame::Days16_64, market_data, interest_rates)?;

        // Calculate consensus score (0-1 based on signal agreement)
        let signals = vec![
            short_term.signal_strength,
            medium_term.signal_strength,
            long_term.signal_strength,
        ];

        // Count how many signals agree on direction (positive vs negative)
        let positive_count = signals.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signals.iter().filter(|&&s| s < 0.0).count();

        let consensus_score = if positive_count == 3 || negative_count == 3 {
            1.0 // Perfect consensus
        } else if positive_count == 2 || negative_count == 2 {
            0.67 // Majority consensus
        } else {
            0.33 // Mixed signals
        };

        // Apply Carver-style consensus boost
        let consensus_boost = if consensus_score > 0.66 {
            1.25 // 25% boost for strong consensus
        } else {
            1.0
        };

        Ok(MultiTimeframeCarry {
            short_term,
            medium_term,
            long_term,
            consensus_score,
            consensus_boost,
        })
    }

    /// Convert carry yield to Carver signal strength (-20 to +20)
    pub fn carry_to_signal_strength(
        &self,
        carry_yield: f64,
        volatility: f64,
        percentile_rank: f64,
    ) -> f64 {
        // Volatility penalty - high volatility reduces signal strength significantly
        let volatility_penalty = if volatility > 0.20 {
            0.3 // Severe penalty for very high volatility
        } else if volatility > 0.15 {
            0.6 // Moderate penalty for high volatility
        } else {
            1.0 // No penalty for normal volatility
        };

        // Base signal strength from carry yield, scaled down by volatility
        let mut signal_strength = carry_yield * 8.0 * volatility_penalty;

        // Apply percentile boost - higher percentiles get stronger signals
        let percentile_boost = 1.0 + (percentile_rank - 0.5) * 0.5;
        signal_strength *= percentile_boost;

        // Clamp to Carver's -20 to +20 range
        signal_strength.clamp(-self.signal_scaling_factor, self.signal_scaling_factor)
    }

    /// Calculate historical carry yield percentiles
    pub fn calculate_carry_percentiles(
        &self,
        _symbol: &str,
        _market_data: &MarketDataHandler,
        current_differential: f64,
    ) -> Result<f64> {
        // Simplified implementation for GREEN phase
        // In real implementation, this would analyze historical rate differentials

        // For now, map current differential to percentile based on typical ranges
        let percentile = if current_differential.abs() < 1.0 {
            0.3 // Low differential = low percentile
        } else if current_differential.abs() < 3.0 {
            0.6 // Medium differential = medium percentile
        } else {
            0.85 // High differential = high percentile
        };

        Ok(percentile)
    }

    /// Determine carry signal type based on differential and market conditions
    pub fn determine_signal_type(&self, differential: f64, volatility: f64) -> CarrySignalType {
        // If differential is below minimum threshold, signal is neutral
        if differential.abs() < self.min_differential_threshold {
            return CarrySignalType::Neutral;
        }

        // If volatility is too high, dampen signal to neutral
        if volatility > self.volatility_threshold {
            return CarrySignalType::Neutral;
        }

        // Otherwise, signal type depends on differential direction
        if differential > 0.0 {
            CarrySignalType::PositiveCarry
        } else {
            CarrySignalType::NegativeCarry
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::MarketDataHandler;
    use chrono::Utc;
    use time;

    fn create_test_market_data() -> MarketDataHandler {
        MarketDataHandler::new()
    }

    fn create_test_interest_rates() -> InterestRateDifferential {
        InterestRateDifferential {
            base_currency: "USD".to_string(),
            quote_currency: "JPY".to_string(),
            base_rate: 5.25,    // USD Fed Funds Rate
            quote_rate: -0.10,  // JPY negative rate
            differential: 5.35, // Positive carry for USD/JPY long
            last_updated: Utc::now(),
        }
    }

    fn create_test_carry_strategy() -> CarryStrategy {
        CarryStrategy::new(
            252,  // 1 year lookback for percentiles
            0.15, // 15% volatility threshold
            0.50, // 50 bps minimum differential
        )
    }

    #[test]
    fn test_carry_signal_positive_differential() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();
        let interest_rates = create_test_interest_rates();

        // Add test price data for USD/JPY
        add_usdjpy_test_data(&mut market_data);

        let signal = strategy.calculate_carry_signal(
            "USD/JPY",
            TimeFrame::Days8_32,
            &market_data,
            &interest_rates,
        )?;

        // Test expectations for positive carry signal
        assert_eq!(signal.symbol, "USD/JPY");
        assert_eq!(signal.timeframe, TimeFrame::Days8_32);
        assert_eq!(signal.signal_type, CarrySignalType::PositiveCarry);
        assert!(
            signal.signal_strength > 0.0,
            "Should be positive carry signal"
        );
        assert!(
            signal.signal_strength <= 20.0,
            "Should not exceed Carver maximum"
        );
        assert_eq!(signal.interest_differential, 5.35);
        assert!(signal.carry_yield_annualized > 0.0);
        assert!(signal.volatility_adjusted_carry > 0.0);
        assert!(signal.percentile_rank >= 0.0 && signal.percentile_rank <= 1.0);

        Ok(())
    }

    #[test]
    fn test_carry_signal_negative_differential() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();

        // Create negative carry scenario (AUD/USD with inverted rates)
        let negative_rates = InterestRateDifferential {
            base_currency: "AUD".to_string(),
            quote_currency: "USD".to_string(),
            base_rate: 1.50,
            quote_rate: 5.25,
            differential: -3.75, // Negative carry for AUD/USD long
            last_updated: Utc::now(),
        };

        add_audusd_test_data(&mut market_data);

        let signal = strategy.calculate_carry_signal(
            "AUD/USD",
            TimeFrame::Days16_64,
            &market_data,
            &negative_rates,
        )?;

        assert_eq!(signal.signal_type, CarrySignalType::NegativeCarry);
        assert!(
            signal.signal_strength < 0.0,
            "Should be negative carry signal"
        );
        assert!(
            signal.signal_strength >= -20.0,
            "Should not exceed Carver minimum"
        );
        assert_eq!(signal.interest_differential, -3.75);

        Ok(())
    }

    #[test]
    fn test_carry_signal_neutral_low_differential() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();

        // Create low differential scenario (below threshold)
        let neutral_rates = InterestRateDifferential {
            base_currency: "EUR".to_string(),
            quote_currency: "GBP".to_string(),
            base_rate: 3.75,
            quote_rate: 3.50,
            differential: 0.25, // Below 50 bps threshold
            last_updated: Utc::now(),
        };

        add_eurgbp_test_data(&mut market_data);

        let signal = strategy.calculate_carry_signal(
            "EUR/GBP",
            TimeFrame::Days4_16,
            &market_data,
            &neutral_rates,
        )?;

        assert_eq!(signal.signal_type, CarrySignalType::Neutral);
        assert!(
            signal.signal_strength.abs() < 2.0,
            "Should be weak/neutral signal"
        );
        assert_eq!(signal.interest_differential, 0.25);

        Ok(())
    }

    #[test]
    fn test_multi_timeframe_carry_consensus() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();
        let interest_rates = create_test_interest_rates();

        add_usdjpy_test_data(&mut market_data);

        let multi_carry =
            strategy.calculate_multi_timeframe_carry("USD/JPY", &market_data, &interest_rates)?;

        // Test multi-timeframe structure
        assert_eq!(multi_carry.short_term.symbol, "USD/JPY");
        assert_eq!(multi_carry.medium_term.symbol, "USD/JPY");
        assert_eq!(multi_carry.long_term.symbol, "USD/JPY");

        // Test consensus scoring
        assert!(multi_carry.consensus_score >= 0.0 && multi_carry.consensus_score <= 1.0);

        // If all timeframes agree on positive carry, should have consensus boost
        if multi_carry.short_term.signal_strength > 0.0
            && multi_carry.medium_term.signal_strength > 0.0
            && multi_carry.long_term.signal_strength > 0.0
        {
            assert!(
                multi_carry.consensus_boost > 1.0,
                "Should boost agreeing signals"
            );
        }

        Ok(())
    }

    #[test]
    fn test_carry_to_signal_strength_conversion() {
        let strategy = create_test_carry_strategy();

        // Test high carry with low volatility -> strong signal
        let strong_signal = strategy.carry_to_signal_strength(4.0, 0.08, 0.85);
        assert!(
            strong_signal > 10.0,
            "High carry/low vol should be strong signal"
        );
        assert!(strong_signal <= 20.0, "Should not exceed Carver maximum");

        // Test low carry with high volatility -> weak signal
        let weak_signal = strategy.carry_to_signal_strength(1.0, 0.25, 0.40);
        assert!(
            weak_signal.abs() < 5.0,
            "Low carry/high vol should be weak signal"
        );

        // Test negative carry -> negative signal
        let negative_signal = strategy.carry_to_signal_strength(-2.5, 0.12, 0.15);
        assert!(
            negative_signal < 0.0,
            "Negative carry should give negative signal"
        );
        assert!(negative_signal >= -20.0, "Should not exceed Carver minimum");
    }

    #[test]
    fn test_carry_percentile_calculation() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();

        add_usdjpy_historical_rates(&mut market_data);

        let percentile = strategy.calculate_carry_percentiles(
            "USD/JPY",
            &market_data,
            5.35, // Current differential
        )?;

        assert!(
            percentile >= 0.0 && percentile <= 1.0,
            "Percentile should be 0-1"
        );

        // With current high differential, should be in upper percentiles
        assert!(
            percentile > 0.7,
            "High differential should be in upper percentiles"
        );

        Ok(())
    }

    #[test]
    fn test_signal_type_determination() {
        let strategy = create_test_carry_strategy();

        // High differential, low volatility -> PositiveCarry
        assert_eq!(
            strategy.determine_signal_type(3.5, 0.10),
            CarrySignalType::PositiveCarry
        );

        // Negative differential -> NegativeCarry
        assert_eq!(
            strategy.determine_signal_type(-2.0, 0.12),
            CarrySignalType::NegativeCarry
        );

        // Small differential -> Neutral
        assert_eq!(
            strategy.determine_signal_type(0.25, 0.08),
            CarrySignalType::Neutral
        );

        // High differential but very high volatility -> might be Neutral
        assert_eq!(
            strategy.determine_signal_type(2.0, 0.35),
            CarrySignalType::Neutral
        );
    }

    #[test]
    fn test_carry_signal_volatility_regime_adjustment() -> Result<()> {
        let strategy = create_test_carry_strategy();
        let mut market_data = create_test_market_data();
        let interest_rates = create_test_interest_rates();

        // Add high volatility regime data
        add_high_volatility_usdjpy_data(&mut market_data);

        let signal = strategy.calculate_carry_signal(
            "USD/JPY",
            TimeFrame::Days8_32,
            &market_data,
            &interest_rates,
        )?;

        // In high volatility regime, signal should be dampened
        assert!(
            signal.regime_adjustment < 1.0,
            "High volatility should dampen signal"
        );
        assert!(
            signal.signal_strength < 15.0,
            "Volatility adjustment should reduce strength"
        );

        Ok(())
    }

    // Helper functions to add test market data
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

    fn add_usdjpy_test_data(market_data: &mut MarketDataHandler) {
        // Add 1 year of USD/JPY price data with steady uptrend
        let base_time = Utc::now() - chrono::Duration::days(365);
        let mut prices = Vec::new();

        for i in 0..365 {
            let time = base_time + chrono::Duration::days(i);
            let price = 110.0 + (i as f64 * 0.02); // Gradual appreciation
            prices.push((time, price));
        }

        add_price_series(market_data, "USD/JPY", prices);
    }

    fn add_audusd_test_data(market_data: &mut MarketDataHandler) {
        // Add AUD/USD test data with downtrend (negative carry scenario)
        let base_time = Utc::now() - chrono::Duration::days(365);
        let mut prices = Vec::new();

        for i in 0..365 {
            let time = base_time + chrono::Duration::days(i);
            let price = 0.7500 - (i as f64 * 0.0002); // Gradual depreciation
            prices.push((time, price));
        }

        add_price_series(market_data, "AUD/USD", prices);
    }

    fn add_eurgbp_test_data(market_data: &mut MarketDataHandler) {
        // Add EUR/GBP neutral/sideways data
        let base_time = Utc::now() - chrono::Duration::days(365);
        let mut prices = Vec::new();

        for i in 0..365 {
            let time = base_time + chrono::Duration::days(i);
            let price = 0.8500 + ((i as f64 * 0.1).sin() * 0.0050); // Sideways oscillation
            prices.push((time, price));
        }

        add_price_series(market_data, "EUR/GBP", prices);
    }

    fn add_usdjpy_historical_rates(_market_data: &mut MarketDataHandler) {
        // This would add historical interest rate differential data
        // For now, we assume the percentile calculation will work with price data
        // In real implementation, this would add rate differential history
    }

    fn add_high_volatility_usdjpy_data(market_data: &mut MarketDataHandler) {
        // Add volatile USD/JPY data to test regime adjustment
        let base_time = Utc::now() - chrono::Duration::days(90);
        let mut prices = Vec::new();

        for i in 0..90 {
            let time = base_time + chrono::Duration::days(i);
            // High volatility price action
            let base_price = 110.0;
            let volatility = ((i as f64 * 0.2).sin() * 5.0) + ((i as f64 * 0.1).cos() * 3.0);
            let price = base_price + volatility;
            prices.push((time, price));
        }

        add_price_series(market_data, "USD/JPY", prices);
    }
}
