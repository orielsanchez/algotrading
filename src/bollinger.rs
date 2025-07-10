use crate::market_data::{MarketDataHandler, TimeFrame};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BollingerBands {
    pub middle_line: f64,   // Simple moving average
    pub upper_band: f64,    // Middle + (std_dev * multiplier)
    pub lower_band: f64,    // Middle - (std_dev * multiplier)
    pub std_deviation: f64, // Standard deviation of prices
    pub bandwidth: f64,     // (Upper - Lower) / Middle
    pub percent_b: f64,     // (Price - Lower) / (Upper - Lower)
}

#[derive(Debug, Clone)]
pub struct BollingerSignal {
    pub symbol: String,
    pub timeframe: TimeFrame,
    pub current_price: f64,
    pub bands: BollingerBands,
    pub signal_strength: f64, // Carver-style -20 to +20
    pub signal_type: BollingerSignalType,
    pub band_squeeze: bool, // Low volatility indicator
}

#[derive(Debug, Clone, PartialEq)]
pub enum BollingerSignalType {
    MeanReversion, // Price near bands, expect reversal
    Breakout,      // Price breaking through bands
    Squeeze,       // Low volatility, expect expansion
    Neutral,       // No clear signal
}

#[derive(Debug, Clone)]
pub struct BollingerMetrics {
    pub timeframe_signals: HashMap<TimeFrame, BollingerSignal>,
    pub composite_signal: f64,
    pub dominant_signal: Option<BollingerSignal>,
    pub volatility_regime: VolatilityRegime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolatilityRegime {
    Low,    // Squeeze conditions
    Normal, // Regular volatility
    High,   // Expansion conditions
}

#[derive(Debug, Clone)]
pub struct BollingerCalculator {
    pub period: usize,          // Moving average period (default 20)
    pub std_multiplier: f64,    // Standard deviation multiplier (default 2.0)
    pub squeeze_threshold: f64, // Bandwidth threshold for squeeze detection
}

impl Default for BollingerCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl BollingerCalculator {
    pub fn new() -> Self {
        Self {
            period: 20,
            std_multiplier: 2.0,
            squeeze_threshold: 0.1, // 10% bandwidth threshold
        }
    }

    pub fn with_settings(period: usize, std_multiplier: f64, squeeze_threshold: f64) -> Self {
        Self {
            period,
            std_multiplier,
            squeeze_threshold,
        }
    }

    /// Calculate Bollinger Bands for a single timeframe
    pub fn calculate_bollinger_bands(&self, prices: &[f64]) -> Option<BollingerBands> {
        if prices.len() < self.period {
            return None;
        }

        // Take the last `period` prices for calculation
        let recent_prices = &prices[prices.len() - self.period..];

        // Calculate simple moving average (middle line)
        let middle_line = recent_prices.iter().sum::<f64>() / recent_prices.len() as f64;

        // Calculate standard deviation
        let variance = recent_prices
            .iter()
            .map(|price| (price - middle_line).powi(2))
            .sum::<f64>()
            / recent_prices.len() as f64;
        let std_deviation = variance.sqrt();

        // Calculate upper and lower bands
        let upper_band = middle_line + (std_deviation * self.std_multiplier);
        let lower_band = middle_line - (std_deviation * self.std_multiplier);

        // Calculate bandwidth (volatility measure)
        let bandwidth = if middle_line != 0.0 {
            (upper_band - lower_band) / middle_line
        } else {
            0.0
        };

        // Calculate %B (price position within bands)
        let current_price = prices.last().copied().unwrap_or(middle_line);
        let percent_b = if upper_band != lower_band {
            (current_price - lower_band) / (upper_band - lower_band)
        } else {
            0.5 // Middle when bands are collapsed
        };

        Some(BollingerBands {
            middle_line,
            upper_band,
            lower_band,
            std_deviation,
            bandwidth,
            percent_b,
        })
    }

    /// Calculate Bollinger signal for a single timeframe
    pub fn calculate_bollinger_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Option<BollingerSignal> {
        let history = market_data.get_price_history(symbol)?;

        if history.prices.len() < self.period {
            return None;
        }

        // Extract just the price values for calculation
        let prices: Vec<f64> = history.prices.iter().map(|(_, price)| *price).collect();
        let current_price = prices.last().copied()?;

        // Calculate Bollinger Bands
        let bands = self.calculate_bollinger_bands(&prices)?;

        // Determine signal type based on price position and band characteristics
        let signal_type = self.determine_signal_type(&bands, current_price);

        // Check for band squeeze (low volatility)
        let band_squeeze = bands.bandwidth < self.squeeze_threshold;

        // Calculate initial signal strength (will be converted to Carver scale later)
        let signal_strength =
            self.calculate_raw_signal_strength(&bands, current_price, &signal_type);

        Some(BollingerSignal {
            symbol: symbol.to_string(),
            timeframe,
            current_price,
            bands,
            signal_strength,
            signal_type,
            band_squeeze,
        })
    }

    /// Determine signal type based on price position relative to bands
    fn determine_signal_type(
        &self,
        bands: &BollingerBands,
        current_price: f64,
    ) -> BollingerSignalType {
        // Check for breakout conditions first (price outside bands)
        if current_price > bands.upper_band {
            return BollingerSignalType::Breakout;
        }

        if current_price < bands.lower_band {
            return BollingerSignalType::Breakout;
        }

        // Check for squeeze condition (very low bandwidth)
        if bands.bandwidth < self.squeeze_threshold {
            return BollingerSignalType::Squeeze;
        }

        // Check for mean reversion conditions (price near bands but not outside)
        if bands.percent_b > 0.8 || bands.percent_b < 0.2 {
            return BollingerSignalType::MeanReversion;
        }

        BollingerSignalType::Neutral
    }

    /// Calculate raw signal strength before Carver conversion
    fn calculate_raw_signal_strength(
        &self,
        bands: &BollingerBands,
        current_price: f64,
        signal_type: &BollingerSignalType,
    ) -> f64 {
        match signal_type {
            BollingerSignalType::MeanReversion => {
                // Mean reversion: opposite to price position
                // %B > 0.5 means price above middle, so negative signal (sell)
                // %B < 0.5 means price below middle, so positive signal (buy)
                let distance_from_middle = bands.percent_b - 0.5;
                -distance_from_middle * 2.0 // Scale to roughly -1 to +1
            }
            BollingerSignalType::Breakout => {
                // Breakout: follow the direction of breakout
                if current_price > bands.upper_band {
                    // Upward breakout - positive signal
                    let breakout_strength =
                        (current_price - bands.upper_band) / bands.std_deviation;
                    breakout_strength.min(1.0)
                } else if current_price < bands.lower_band {
                    // Downward breakout - negative signal
                    let breakout_strength =
                        (bands.lower_band - current_price) / bands.std_deviation;
                    -breakout_strength.min(1.0)
                } else {
                    0.0
                }
            }
            BollingerSignalType::Squeeze => {
                // Squeeze: minimal signal, preparing for expansion
                0.1 // Small positive signal indicating potential opportunity
            }
            BollingerSignalType::Neutral => 0.0,
        }
    }

    /// Calculate multi-timeframe Bollinger analysis
    pub fn calculate_multi_timeframe_bollinger(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Option<BollingerMetrics> {
        let timeframes = TimeFrame::carver_momentum_timeframes();
        let mut timeframe_signals = HashMap::new();
        let mut signal_strengths = Vec::new();

        for timeframe in timeframes {
            if let Some(signal) = self.calculate_bollinger_signal(symbol, timeframe, market_data) {
                // Convert to Carver signal strength
                let carver_strength = self.bollinger_to_signal_strength(&signal);
                let mut signal_with_carver = signal;
                signal_with_carver.signal_strength = carver_strength;

                signal_strengths.push(carver_strength);
                timeframe_signals.insert(timeframe, signal_with_carver);
            }
        }

        if timeframe_signals.is_empty() {
            return None;
        }

        // Calculate composite signal as weighted average
        let composite_signal = signal_strengths.iter().sum::<f64>() / signal_strengths.len() as f64;

        // Find dominant signal (strongest absolute value)
        let dominant_signal = timeframe_signals
            .values()
            .max_by(|a, b| {
                a.signal_strength
                    .abs()
                    .partial_cmp(&b.signal_strength.abs())
                    .unwrap()
            })
            .cloned();

        // Determine volatility regime based on band squeeze conditions
        let squeeze_count = timeframe_signals
            .values()
            .filter(|s| s.band_squeeze)
            .count();
        let total_signals = timeframe_signals.len();

        let volatility_regime = if squeeze_count > total_signals / 2 {
            VolatilityRegime::Low
        } else {
            // Check average bandwidth across timeframes
            let avg_bandwidth: f64 = timeframe_signals
                .values()
                .map(|s| s.bands.bandwidth)
                .sum::<f64>()
                / total_signals as f64;

            if avg_bandwidth > 0.3 {
                VolatilityRegime::High
            } else {
                VolatilityRegime::Normal
            }
        };

        Some(BollingerMetrics {
            timeframe_signals,
            composite_signal,
            dominant_signal,
            volatility_regime,
        })
    }

    /// Convert Bollinger signal to Carver-style signal strength (-20 to +20)
    pub fn bollinger_to_signal_strength(&self, signal: &BollingerSignal) -> f64 {
        // Base signal strength from raw calculation
        let base_strength = signal.signal_strength;

        // Apply signal type multipliers
        let type_multiplier = match signal.signal_type {
            BollingerSignalType::MeanReversion => {
                // Mean reversion signals are typically stronger when %B is extreme
                let extremeness = (signal.bands.percent_b - 0.5).abs() * 2.0; // 0 to 1
                1.0 + extremeness // 1.0 to 2.0 multiplier
            }
            BollingerSignalType::Breakout => {
                // Breakout signals are stronger when significantly outside bands
                let breakout_distance = if signal.current_price > signal.bands.upper_band {
                    (signal.current_price - signal.bands.upper_band) / signal.bands.std_deviation
                } else if signal.current_price < signal.bands.lower_band {
                    (signal.bands.lower_band - signal.current_price) / signal.bands.std_deviation
                } else {
                    0.0
                };
                1.0 + breakout_distance.min(1.0) // 1.0 to 2.0 multiplier
            }
            BollingerSignalType::Squeeze => {
                // Squeeze signals are weaker but important for timing
                0.5 // Reduced multiplier
            }
            BollingerSignalType::Neutral => 0.1, // Very weak signal
        };

        // Apply bandwidth adjustment (higher volatility = stronger signals)
        let volatility_multiplier = if signal.bands.bandwidth > 0.0 {
            // Normalize bandwidth to reasonable range
            let normalized_bandwidth = signal.bands.bandwidth.min(0.5) / 0.5; // 0 to 1
            0.5 + normalized_bandwidth * 0.5 // 0.5 to 1.0 multiplier
        } else {
            0.5 // Low volatility
        };

        // Combine all factors and scale to Carver range
        let combined_strength = base_strength * type_multiplier * volatility_multiplier;
        let carver_strength = combined_strength * 20.0; // Scale to -20 to +20

        // Ensure within bounds
        carver_strength.clamp(-20.0, 20.0)
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
    fn test_bollinger_calculator_creation() {
        let calc = BollingerCalculator::new();
        assert_eq!(calc.period, 20);
        assert_eq!(calc.std_multiplier, 2.0);
        assert_eq!(calc.squeeze_threshold, 0.1);
    }

    #[test]
    fn test_bollinger_calculator_with_custom_settings() {
        let calc = BollingerCalculator::with_settings(14, 1.5, 0.05);
        assert_eq!(calc.period, 14);
        assert_eq!(calc.std_multiplier, 1.5);
        assert_eq!(calc.squeeze_threshold, 0.05);
    }

    #[test]
    fn test_basic_bollinger_bands_calculation() {
        let calc = BollingerCalculator::with_settings(5, 2.0, 0.1); // Use 5-period for this test

        // Test with simple price series: 18, 19, 20, 21, 22
        // Mean should be 20, std dev should be ~1.58
        let prices = vec![18.0, 19.0, 20.0, 21.0, 22.0];
        let bands = calc
            .calculate_bollinger_bands(&prices)
            .expect("Should calculate bands");

        assert!((bands.middle_line - 20.0).abs() < 0.01);
        assert!(bands.upper_band > bands.middle_line);
        assert!(bands.lower_band < bands.middle_line);
        assert!(bands.std_deviation > 0.0);
        assert!(bands.bandwidth > 0.0);
    }

    #[test]
    fn test_bollinger_bands_with_real_data() {
        let calc = BollingerCalculator::new();

        // Test with 25 data points for proper 20-period calculation
        let prices: Vec<f64> = (1..=25).map(|i| 100.0 + (i as f64) * 0.5).collect();
        let bands = calc
            .calculate_bollinger_bands(&prices)
            .expect("Should calculate bands");

        // With trending data, bands should be wider
        assert!(bands.upper_band > bands.middle_line + 1.0);
        assert!(bands.lower_band < bands.middle_line - 1.0);
        assert!(bands.bandwidth > 0.05); // Should have reasonable width
    }

    #[test]
    fn test_bollinger_bands_with_constant_prices() {
        let calc = BollingerCalculator::new();

        // All prices are the same - should have zero standard deviation
        let prices = vec![100.0; 25];
        let bands = calc
            .calculate_bollinger_bands(&prices)
            .expect("Should calculate bands");

        assert!((bands.middle_line - 100.0).abs() < 0.01);
        assert!((bands.upper_band - 100.0).abs() < 0.01);
        assert!((bands.lower_band - 100.0).abs() < 0.01);
        assert!(bands.std_deviation < 0.01); // Should be near zero
        assert!(bands.bandwidth < 0.01); // Should be near zero
    }

    #[test]
    fn test_insufficient_data_handling() {
        let calc = BollingerCalculator::new();

        // Not enough data for 20-period calculation
        let prices = vec![100.0, 101.0, 102.0];
        let bands = calc.calculate_bollinger_bands(&prices);

        assert!(bands.is_none()); // Should return None for insufficient data
    }

    #[test]
    fn test_mean_reversion_signal_generation() {
        let mut market_data = create_test_market_data();
        let calc = BollingerCalculator::new();

        // Create a very simple price series that will create predictable bands
        let base_time = Utc::now();
        let mut prices = Vec::new();

        // 20 prices exactly at 100 (will create zero std deviation initially)
        for i in 0..20 {
            prices.push((base_time - chrono::Duration::days(20 - i), 100.0));
        }
        // Add some small variation to create bands
        for i in 20..24 {
            let price = 100.0 + ((i % 2) as f64 - 0.5) * 0.1; // Very small variation
            prices.push((base_time - chrono::Duration::days(24 - i), price));
        }
        // Price that should be near upper band for mean reversion
        prices.push((base_time, 100.15)); // Small move up

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_bollinger_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();

        // If the price is still above the upper band, it's a breakout, otherwise mean reversion
        if signal.current_price > signal.bands.upper_band {
            assert_eq!(signal.signal_type, BollingerSignalType::Breakout);
        } else {
            assert_eq!(signal.signal_type, BollingerSignalType::MeanReversion);
            assert!(signal.signal_strength < 0.0); // Should be negative (sell signal)
        }
    }

    #[test]
    fn test_breakout_signal_generation() {
        let mut market_data = create_test_market_data();
        let calc = BollingerCalculator::new();

        // Create price series with clear breakout above upper band
        let base_time = Utc::now();
        let mut prices = Vec::new();

        // 25 prices in tight range around 100, then clear breakout
        for i in 0..24 {
            let price = 100.0 + ((i % 3) as f64) * 0.1; // Sideways movement ±0.2
            prices.push((base_time - chrono::Duration::days(24 - i), price));
        }
        // Strong breakout above upper band
        prices.push((base_time, 108.0)); // Should clearly exceed upper band

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_bollinger_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal_type, BollingerSignalType::Breakout);
        assert!(signal.signal_strength > 0.0); // Should be positive (buy signal)
        assert!(signal.bands.percent_b > 1.0); // Should be above upper band
    }

    #[test]
    fn test_squeeze_detection() {
        let mut market_data = create_test_market_data();
        let calc = BollingerCalculator::with_settings(20, 2.0, 0.05); // Lower threshold

        // Create very tight price range (low volatility)
        let base_time = Utc::now();
        let mut prices = Vec::new();

        for i in 0..25 {
            let price = 100.0 + ((i % 2) as f64) * 0.01; // Very tight range
            prices.push((base_time - chrono::Duration::days(25 - i), price));
        }

        add_price_series(&mut market_data, "AAPL", prices);

        let signal = calc.calculate_bollinger_signal("AAPL", TimeFrame::Days1, &market_data);

        assert!(signal.is_some());
        let signal = signal.unwrap();
        assert_eq!(signal.signal_type, BollingerSignalType::Squeeze);
        assert!(signal.band_squeeze);
        assert!(signal.bands.bandwidth < 0.05); // Should be very narrow
    }

    #[test]
    fn test_multi_timeframe_bollinger_analysis() {
        let mut market_data = create_test_market_data();
        let calc = BollingerCalculator::new();

        // Create comprehensive price series for multi-timeframe analysis
        let base_time = Utc::now();
        let mut prices = Vec::new();

        // 70 days of data for longer timeframes
        for i in 0..70 {
            let price = 100.0 + (i as f64) * 0.1 + ((i % 5) as f64) * 0.2; // Trend + noise
            prices.push((base_time - chrono::Duration::days(70 - i), price));
        }

        add_price_series(&mut market_data, "AAPL", prices);

        let metrics = calc.calculate_multi_timeframe_bollinger("AAPL", &market_data);

        assert!(metrics.is_some());
        let metrics = metrics.unwrap();
        assert!(!metrics.timeframe_signals.is_empty());
        assert!(metrics.composite_signal.abs() <= 20.0); // Should be within Carver range
        assert!(metrics.dominant_signal.is_some());
    }

    #[test]
    fn test_signal_strength_conversion() {
        let calc = BollingerCalculator::new();

        // Test strong mean reversion signal (price at upper band)
        let mean_reversion_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            current_price: 109.0, // Just below upper band
            bands: BollingerBands {
                middle_line: 100.0,
                upper_band: 110.0,
                lower_band: 90.0,
                std_deviation: 5.0,
                bandwidth: 0.2,
                percent_b: 0.9, // 90% up the band (mean reversion territory)
            },
            signal_strength: -0.8, // Strong negative signal (expect price to revert)
            signal_type: BollingerSignalType::MeanReversion,
            band_squeeze: false,
        };

        let strength = calc.bollinger_to_signal_strength(&mean_reversion_signal);
        assert!(strength < 0.0); // Should be negative (sell signal)
        assert!((-20.0..=20.0).contains(&strength)); // Within Carver range

        // Test breakout signal (price above upper band)
        let breakout_signal = BollingerSignal {
            symbol: "AAPL".to_string(),
            timeframe: TimeFrame::Days1,
            current_price: 115.0,
            bands: BollingerBands {
                middle_line: 100.0,
                upper_band: 110.0,
                lower_band: 90.0,
                std_deviation: 5.0,
                bandwidth: 0.2,
                percent_b: 1.25, // Above upper band
            },
            signal_strength: 1.0, // Positive raw signal (upward breakout)
            signal_type: BollingerSignalType::Breakout,
            band_squeeze: false,
        };

        let strength = calc.bollinger_to_signal_strength(&breakout_signal);
        assert!(strength > 0.0); // Should be positive (buy signal)
        assert!((-20.0..=20.0).contains(&strength)); // Within Carver range
    }

    #[test]
    fn test_volatility_regime_detection() {
        let calc = BollingerCalculator::new();

        // Test will verify that we can detect different volatility regimes
        // based on bandwidth and recent price action
        let mut market_data = create_test_market_data();

        // High volatility scenario - create wide price swings
        let base_time = Utc::now();
        let mut high_vol_prices = Vec::new();
        for i in 0..25 {
            let price = 100.0 + ((i % 4) as f64 - 2.0) * 10.0; // Wide swings: 80 to 120
            high_vol_prices.push((base_time - chrono::Duration::days(25 - i), price));
        }

        add_price_series(&mut market_data, "HVOL", high_vol_prices);

        let metrics = calc.calculate_multi_timeframe_bollinger("HVOL", &market_data);
        assert!(metrics.is_some());
        let metrics = metrics.unwrap();
        assert_eq!(metrics.volatility_regime, VolatilityRegime::High);
    }
}
