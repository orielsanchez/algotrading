use crate::market_data::{MarketDataHandler, TimeFrame};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BreakoutSignal {
    pub symbol: String,
    pub timeframe: TimeFrame,
    pub signal_strength: f64,
    pub breakout_type: BreakoutType,
    pub current_price: f64,
    pub breakout_level: f64,
    pub lookback_high: f64,
    pub lookback_low: f64,
    pub volatility_normalized: f64,
    pub percentile_rank: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BreakoutType {
    UpBreakout,   // Price breaks above recent high
    DownBreakout, // Price breaks below recent low
    NoBreakout,   // No significant breakout detected
}

#[derive(Debug, Clone)]
pub struct BreakoutMetrics {
    pub timeframe_signals: HashMap<TimeFrame, BreakoutSignal>,
    pub composite_signal: f64,
    pub strongest_signal: Option<BreakoutSignal>,
    pub consensus_strength: f64,
}

#[derive(Debug, Clone)]
pub struct BreakoutCalculator {
    /// Minimum percentage move required to consider a breakout
    pub min_breakout_threshold: f64,
    /// Standard deviation multiplier for volatility-adjusted breakouts
    pub volatility_multiplier: f64,
    /// Lookback periods for breakout detection
    pub lookback_periods: Vec<usize>,
}

impl Default for BreakoutCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl BreakoutCalculator {
    pub fn new() -> Self {
        Self {
            min_breakout_threshold: 0.01,            // 1% minimum breakout
            volatility_multiplier: 1.5,              // 1.5x volatility for breakout threshold
            lookback_periods: vec![10, 20, 50, 100], // Common breakout periods
        }
    }

    pub fn with_settings(
        min_threshold: f64,
        vol_multiplier: f64,
        lookback_periods: Vec<usize>,
    ) -> Self {
        Self {
            min_breakout_threshold: min_threshold,
            volatility_multiplier: vol_multiplier,
            lookback_periods,
        }
    }

    /// Calculate breakout signal for a single timeframe
    pub fn calculate_breakout_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Option<BreakoutSignal> {
        let history = market_data.get_price_history(symbol)?;

        if history.prices.len() < 10 {
            return None; // Need at least 10 price points
        }

        let current_price = history.prices.last()?.1;
        let lookback_period = self.get_lookback_period_for_timeframe(timeframe);

        if history.prices.len() < lookback_period {
            return None;
        }

        // Get relevant price slice for lookback period (excluding current price)
        let lookback_prices: Vec<f64> = history
            .prices
            .iter()
            .rev()
            .skip(1) // Skip current price
            .take(lookback_period)
            .map(|(_, price)| *price)
            .collect();

        if lookback_prices.is_empty() {
            return None;
        }

        let lookback_high = lookback_prices
            .iter()
            .fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let lookback_low = lookback_prices.iter().fold(f64::INFINITY, |a, &b| a.min(b));

        // Calculate volatility for normalization
        let volatility = self.calculate_volatility(&lookback_prices);

        // Determine breakout type and level
        let (breakout_type, breakout_level) =
            self.determine_breakout_type(current_price, lookback_high, lookback_low, volatility);

        // Calculate percentile rank
        let percentile_rank = self.calculate_percentile_rank(current_price, &lookback_prices);

        // Calculate raw signal strength
        let raw_signal_strength = self.calculate_raw_signal_strength(
            current_price,
            breakout_level,
            lookback_high,
            lookback_low,
            &breakout_type,
        );

        // Normalize for volatility
        let volatility_normalized = if volatility > 0.0 {
            raw_signal_strength / volatility
        } else {
            raw_signal_strength
        };

        // Create temporary signal to convert to proper signal strength
        let temp_signal = BreakoutSignal {
            symbol: symbol.to_string(),
            timeframe,
            signal_strength: raw_signal_strength,
            breakout_type: breakout_type.clone(),
            current_price,
            breakout_level,
            lookback_high,
            lookback_low,
            volatility_normalized,
            percentile_rank,
        };

        // Convert to Carver-style signal strength (-20 to +20)
        let carver_signal_strength = self.breakout_to_signal_strength(&temp_signal);

        Some(BreakoutSignal {
            symbol: symbol.to_string(),
            timeframe,
            signal_strength: carver_signal_strength,
            breakout_type,
            current_price,
            breakout_level,
            lookback_high,
            lookback_low,
            volatility_normalized,
            percentile_rank,
        })
    }

    /// Calculate breakout signals across multiple timeframes
    pub fn calculate_multi_timeframe_breakout(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Option<BreakoutMetrics> {
        let timeframes = TimeFrame::carver_momentum_timeframes();
        let mut timeframe_signals = HashMap::new();
        let mut signal_strengths = Vec::new();

        for timeframe in timeframes {
            if let Some(signal) = self.calculate_breakout_signal(symbol, timeframe, market_data) {
                signal_strengths.push(signal.signal_strength);
                timeframe_signals.insert(timeframe, signal);
            }
        }

        if timeframe_signals.is_empty() {
            return None;
        }

        // Calculate composite signal as weighted average
        let composite_signal = signal_strengths.iter().sum::<f64>() / signal_strengths.len() as f64;

        // Find strongest signal
        let strongest_signal = timeframe_signals
            .values()
            .max_by(|a, b| {
                a.signal_strength
                    .abs()
                    .partial_cmp(&b.signal_strength.abs())
                    .unwrap()
            })
            .cloned();

        // Calculate consensus strength (how many timeframes agree)
        let positive_signals = signal_strengths.iter().filter(|&&s| s > 0.0).count();
        let negative_signals = signal_strengths.iter().filter(|&&s| s < 0.0).count();
        let total_signals = signal_strengths.len();

        let consensus_strength = if total_signals > 0 {
            let max_agreement = positive_signals.max(negative_signals);
            max_agreement as f64 / total_signals as f64
        } else {
            0.0
        };

        Some(BreakoutMetrics {
            timeframe_signals,
            composite_signal,
            strongest_signal,
            consensus_strength,
        })
    }

    /// Convert breakout signal to Carver-style signal strength (-20 to +20)
    pub fn breakout_to_signal_strength(&self, breakout: &BreakoutSignal) -> f64 {
        match breakout.breakout_type {
            BreakoutType::NoBreakout => 0.0,
            BreakoutType::UpBreakout => {
                // Calculate strength based on breakout magnitude and percentile rank
                let magnitude =
                    (breakout.current_price - breakout.breakout_level) / breakout.breakout_level;
                let base_strength = magnitude * 100.0; // Convert to percentage
                let percentile_boost = (breakout.percentile_rank - 0.5) * 2.0; // -1 to +1
                let final_strength = base_strength * 20.0 + percentile_boost * 10.0;
                final_strength.clamp(0.0, 20.0)
            }
            BreakoutType::DownBreakout => {
                // Calculate strength based on breakdown magnitude and percentile rank
                let magnitude =
                    (breakout.breakout_level - breakout.current_price) / breakout.breakout_level;
                let base_strength = magnitude * 100.0; // Convert to percentage
                let percentile_boost = (0.5 - breakout.percentile_rank) * 2.0; // -1 to +1
                let final_strength = -(base_strength * 20.0 + percentile_boost * 10.0);
                final_strength.clamp(-20.0, 0.0)
            }
        }
    }

    // Helper methods
    fn get_lookback_period_for_timeframe(&self, timeframe: TimeFrame) -> usize {
        match timeframe {
            TimeFrame::Days2_8 => 8,
            TimeFrame::Days4_16 => 16,
            TimeFrame::Days8_32 => 32,
            TimeFrame::Days16_64 => 64,
            _ => 20, // Default lookback
        }
    }

    fn calculate_volatility(&self, prices: &[f64]) -> f64 {
        if prices.len() < 2 {
            return 0.0;
        }

        let returns: Vec<f64> = prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();

        if returns.is_empty() {
            return 0.0;
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

        variance.sqrt()
    }

    fn determine_breakout_type(
        &self,
        current_price: f64,
        lookback_high: f64,
        lookback_low: f64,
        volatility: f64,
    ) -> (BreakoutType, f64) {
        let vol_threshold = volatility * self.volatility_multiplier;
        let min_threshold = self.min_breakout_threshold;

        // Check for upward breakout
        if current_price > lookback_high {
            let breakout_magnitude = (current_price - lookback_high) / lookback_high;
            if breakout_magnitude > min_threshold || breakout_magnitude > vol_threshold {
                return (BreakoutType::UpBreakout, lookback_high);
            }
        }

        // Check for downward breakout
        if current_price < lookback_low {
            let breakout_magnitude = (lookback_low - current_price) / lookback_low;
            if breakout_magnitude > min_threshold || breakout_magnitude > vol_threshold {
                return (BreakoutType::DownBreakout, lookback_low);
            }
        }

        // No significant breakout
        (BreakoutType::NoBreakout, current_price)
    }

    fn calculate_percentile_rank(&self, current_price: f64, prices: &[f64]) -> f64 {
        if prices.is_empty() {
            return 0.5;
        }

        let below_count = prices.iter().filter(|&&p| p < current_price).count();
        below_count as f64 / prices.len() as f64
    }

    fn calculate_raw_signal_strength(
        &self,
        current_price: f64,
        breakout_level: f64,
        lookback_high: f64,
        lookback_low: f64,
        breakout_type: &BreakoutType,
    ) -> f64 {
        let range = lookback_high - lookback_low;
        if range == 0.0 {
            return 0.0;
        }

        match breakout_type {
            BreakoutType::NoBreakout => 0.0,
            BreakoutType::UpBreakout => {
                let breakout_distance = current_price - breakout_level;
                (breakout_distance / range).clamp(0.0, 1.0)
            }
            BreakoutType::DownBreakout => {
                let breakout_distance = breakout_level - current_price;
                -(breakout_distance / range).clamp(0.0, 1.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::MarketDataHandler;
    use chrono::{DateTime, Utc};

    fn create_test_market_data() -> MarketDataHandler {
        MarketDataHandler::new()
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
    fn test_breakout_calculator_creation() {
        let calc = BreakoutCalculator::new();
        assert_eq!(calc.min_breakout_threshold, 0.01); // Updated to match implementation
        assert_eq!(calc.volatility_multiplier, 1.5);
        assert_eq!(calc.lookback_periods.len(), 4);
    }

    #[test]
    fn test_breakout_calculator_with_custom_settings() {
        let calc = BreakoutCalculator::with_settings(0.03, 2.0, vec![5, 10, 20]);
        assert_eq!(calc.min_breakout_threshold, 0.03);
        assert_eq!(calc.volatility_multiplier, 2.0);
        assert_eq!(calc.lookback_periods, vec![5, 10, 20]);
    }

    #[test]
    fn test_upward_breakout_detection() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Create upward trending price series with clear breakout
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(20), 100.0),
            (base_time - chrono::Duration::days(19), 101.0),
            (base_time - chrono::Duration::days(18), 102.0),
            (base_time - chrono::Duration::days(17), 103.0),
            (base_time - chrono::Duration::days(16), 104.0),
            (base_time - chrono::Duration::days(15), 105.0), // Previous high
            (base_time - chrono::Duration::days(14), 104.0),
            (base_time - chrono::Duration::days(13), 103.0),
            (base_time - chrono::Duration::days(12), 102.0),
            (base_time - chrono::Duration::days(11), 101.0),
            (base_time - chrono::Duration::days(10), 100.0),
            (base_time - chrono::Duration::days(9), 99.0),
            (base_time - chrono::Duration::days(8), 98.0),
            (base_time - chrono::Duration::days(7), 99.0),
            (base_time - chrono::Duration::days(6), 100.0),
            (base_time - chrono::Duration::days(5), 101.0),
            (base_time - chrono::Duration::days(4), 102.0),
            (base_time - chrono::Duration::days(3), 103.0),
            (base_time - chrono::Duration::days(2), 104.0),
            (base_time - chrono::Duration::days(1), 105.0),
            (base_time, 108.0), // Clear breakout above 105
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.breakout_type, BreakoutType::UpBreakout);
        assert_eq!(signal.current_price, 108.0);
        assert_eq!(signal.breakout_level, 105.0);
        assert!(signal.signal_strength > 0.0);
        assert!(signal.percentile_rank > 0.8); // Should be high percentile
    }

    #[test]
    fn test_downward_breakout_detection() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Create downward trending price series with clear breakdown
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(20), 100.0),
            (base_time - chrono::Duration::days(19), 99.0),
            (base_time - chrono::Duration::days(18), 98.0),
            (base_time - chrono::Duration::days(17), 97.0),
            (base_time - chrono::Duration::days(16), 96.0),
            (base_time - chrono::Duration::days(15), 95.0), // Previous low
            (base_time - chrono::Duration::days(14), 96.0),
            (base_time - chrono::Duration::days(13), 97.0),
            (base_time - chrono::Duration::days(12), 98.0),
            (base_time - chrono::Duration::days(11), 99.0),
            (base_time - chrono::Duration::days(10), 100.0),
            (base_time - chrono::Duration::days(9), 101.0),
            (base_time - chrono::Duration::days(8), 102.0),
            (base_time - chrono::Duration::days(7), 101.0),
            (base_time - chrono::Duration::days(6), 100.0),
            (base_time - chrono::Duration::days(5), 99.0),
            (base_time - chrono::Duration::days(4), 98.0),
            (base_time - chrono::Duration::days(3), 97.0),
            (base_time - chrono::Duration::days(2), 96.0),
            (base_time - chrono::Duration::days(1), 95.0),
            (base_time, 92.0), // Clear breakdown below 95
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.breakout_type, BreakoutType::DownBreakout);
        assert_eq!(signal.current_price, 92.0);
        assert_eq!(signal.breakout_level, 95.0);
        assert!(signal.signal_strength < 0.0);
        assert!(signal.percentile_rank < 0.2); // Should be low percentile
    }

    #[test]
    fn test_no_breakout_detection() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Create sideways price series with no clear breakout
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(20), 100.0),
            (base_time - chrono::Duration::days(19), 101.0),
            (base_time - chrono::Duration::days(18), 99.0),
            (base_time - chrono::Duration::days(17), 100.5),
            (base_time - chrono::Duration::days(16), 99.5),
            (base_time - chrono::Duration::days(15), 100.0),
            (base_time - chrono::Duration::days(14), 101.0),
            (base_time - chrono::Duration::days(13), 99.0),
            (base_time - chrono::Duration::days(12), 100.0),
            (base_time - chrono::Duration::days(11), 99.5),
            (base_time - chrono::Duration::days(10), 100.5),
            (base_time - chrono::Duration::days(9), 100.0),
            (base_time - chrono::Duration::days(8), 99.0),
            (base_time - chrono::Duration::days(7), 101.0),
            (base_time - chrono::Duration::days(6), 100.0),
            (base_time - chrono::Duration::days(5), 99.5),
            (base_time - chrono::Duration::days(4), 100.5),
            (base_time - chrono::Duration::days(3), 100.0),
            (base_time - chrono::Duration::days(2), 99.0),
            (base_time - chrono::Duration::days(1), 101.0),
            (base_time, 100.0), // No significant breakout
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.breakout_type, BreakoutType::NoBreakout);
        assert_eq!(signal.current_price, 100.0);
        assert!(signal.signal_strength.abs() < 0.1); // Should be near zero
    }

    #[test]
    fn test_volatility_adjusted_breakout() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Create high volatility price series
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(20), 100.0),
            (base_time - chrono::Duration::days(19), 110.0),
            (base_time - chrono::Duration::days(18), 90.0),
            (base_time - chrono::Duration::days(17), 105.0),
            (base_time - chrono::Duration::days(16), 95.0),
            (base_time - chrono::Duration::days(15), 115.0), // Volatile high
            (base_time - chrono::Duration::days(14), 85.0),
            (base_time - chrono::Duration::days(13), 110.0),
            (base_time - chrono::Duration::days(12), 90.0),
            (base_time - chrono::Duration::days(11), 105.0),
            (base_time - chrono::Duration::days(10), 95.0),
            (base_time - chrono::Duration::days(9), 100.0),
            (base_time - chrono::Duration::days(8), 110.0),
            (base_time - chrono::Duration::days(7), 90.0),
            (base_time - chrono::Duration::days(6), 105.0),
            (base_time - chrono::Duration::days(5), 95.0),
            (base_time - chrono::Duration::days(4), 108.0),
            (base_time - chrono::Duration::days(3), 92.0),
            (base_time - chrono::Duration::days(2), 107.0),
            (base_time - chrono::Duration::days(1), 93.0),
            (base_time, 118.0), // Should be volatility-adjusted
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert!(signal.volatility_normalized > 0.0);
        // In high volatility, breakouts should be adjusted but may still be strong
        assert!(signal.signal_strength > 0.0 && signal.signal_strength <= 20.0); // Should be positive and within range
    }

    #[test]
    fn test_multi_timeframe_breakout_consensus() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Create price series that shows breakout across multiple timeframes
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(65), 90.0),
            (base_time - chrono::Duration::days(60), 95.0),
            (base_time - chrono::Duration::days(55), 100.0),
            (base_time - chrono::Duration::days(50), 105.0),
            (base_time - chrono::Duration::days(45), 110.0),
            (base_time - chrono::Duration::days(40), 115.0), // Long-term high
            (base_time - chrono::Duration::days(35), 110.0),
            (base_time - chrono::Duration::days(30), 105.0),
            (base_time - chrono::Duration::days(25), 100.0),
            (base_time - chrono::Duration::days(20), 95.0),
            (base_time - chrono::Duration::days(15), 105.0), // Medium-term high
            (base_time - chrono::Duration::days(10), 100.0),
            (base_time - chrono::Duration::days(8), 105.0),
            (base_time - chrono::Duration::days(6), 110.0),
            (base_time - chrono::Duration::days(4), 115.0),
            (base_time - chrono::Duration::days(2), 118.0), // Breaking multiple levels
            (base_time, 120.0),                             // Strong breakout
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let metrics = calc.calculate_multi_timeframe_breakout("AAPL", &market_data);

        assert!(metrics.is_some());
        let metrics = metrics.unwrap();
        assert!(metrics.composite_signal > 0.0);
        assert!(metrics.consensus_strength > 0.5); // Should show strong consensus
        assert!(metrics.strongest_signal.is_some());

        let strongest = metrics.strongest_signal.unwrap();
        assert_eq!(strongest.breakout_type, BreakoutType::UpBreakout);
        assert!(strongest.signal_strength > 10.0); // Should be strong signal
    }

    #[test]
    fn test_signal_strength_conversion() {
        let calc = BreakoutCalculator::new();

        // Test strong upward breakout
        let strong_up_signal = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0, // Will be calculated
            breakout_type: BreakoutType::UpBreakout,
            current_price: 110.0,
            breakout_level: 100.0,
            lookback_high: 100.0,
            lookback_low: 90.0,
            volatility_normalized: 0.8,
            percentile_rank: 0.95,
        };

        let strength = calc.breakout_to_signal_strength(&strong_up_signal);
        assert!(strength > 10.0 && strength <= 20.0); // Should be strong positive

        // Test strong downward breakout
        let strong_down_signal = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0,
            breakout_type: BreakoutType::DownBreakout,
            current_price: 90.0,
            breakout_level: 100.0,
            lookback_high: 110.0,
            lookback_low: 100.0,
            volatility_normalized: 0.8,
            percentile_rank: 0.05,
        };

        let strength = calc.breakout_to_signal_strength(&strong_down_signal);
        assert!((-20.0..-10.0).contains(&strength)); // Should be strong negative

        // Test no breakout
        let no_signal = BreakoutSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            signal_strength: 0.0,
            breakout_type: BreakoutType::NoBreakout,
            current_price: 100.0,
            breakout_level: 100.0,
            lookback_high: 110.0,
            lookback_low: 90.0,
            volatility_normalized: 0.0,
            percentile_rank: 0.5,
        };

        let strength = calc.breakout_to_signal_strength(&no_signal);
        assert!(strength.abs() < 1.0); // Should be near zero
    }

    #[test]
    fn test_insufficient_data_handling() {
        let mut market_data = create_test_market_data();
        let calc = BreakoutCalculator::new();

        // Add only a few price points - insufficient for breakout calculation
        let base_time = Utc::now();
        let prices = vec![
            (base_time - chrono::Duration::days(2), 100.0),
            (base_time - chrono::Duration::days(1), 101.0),
            (base_time, 102.0),
        ];

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_breakout_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_none()); // Should return None for insufficient data
    }
}
