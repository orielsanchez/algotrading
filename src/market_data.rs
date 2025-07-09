use crate::security_types::SecurityInfo;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeFrame {
    Minutes15,
    Hours1,
    Hours4,
    Days1,
    Days7,
    Days14,
    // Carver's multi-timeframe momentum periods
    Days2_8,    // 2-8 day momentum
    Days4_16,   // 4-16 day momentum  
    Days8_32,   // 8-32 day momentum
    Days16_64,  // 16-64 day momentum
}

impl TimeFrame {
    pub fn to_minutes(self) -> i64 {
        match self {
            TimeFrame::Minutes15 => 15,
            TimeFrame::Hours1 => 60,
            TimeFrame::Hours4 => 240,
            TimeFrame::Days1 => 1440,
            TimeFrame::Days7 => 10080,
            TimeFrame::Days14 => 20160,
            // For Carver's momentum periods, use the midpoint of the range
            TimeFrame::Days2_8 => 5 * 1440,    // 5 days
            TimeFrame::Days4_16 => 10 * 1440,  // 10 days
            TimeFrame::Days8_32 => 20 * 1440,  // 20 days
            TimeFrame::Days16_64 => 40 * 1440, // 40 days
        }
    }

    pub fn to_duration(self) -> Duration {
        Duration::minutes(self.to_minutes())
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimeFrame::Minutes15 => "15m",
            TimeFrame::Hours1 => "1h",
            TimeFrame::Hours4 => "4h",
            TimeFrame::Days1 => "1d",
            TimeFrame::Days7 => "7d",
            TimeFrame::Days14 => "14d",
            TimeFrame::Days2_8 => "2-8d",
            TimeFrame::Days4_16 => "4-16d",
            TimeFrame::Days8_32 => "8-32d",
            TimeFrame::Days16_64 => "16-64d",
        }
    }

    pub fn all_timeframes() -> Vec<TimeFrame> {
        vec![
            TimeFrame::Minutes15,
            TimeFrame::Hours1,
            TimeFrame::Hours4,
            TimeFrame::Days1,
            TimeFrame::Days7,
            TimeFrame::Days14,
            TimeFrame::Days2_8,
            TimeFrame::Days4_16,
            TimeFrame::Days8_32,
            TimeFrame::Days16_64,
        ]
    }
    
    /// Get Carver's multi-timeframe momentum periods
    pub fn carver_momentum_timeframes() -> Vec<TimeFrame> {
        vec![
            TimeFrame::Days2_8,
            TimeFrame::Days4_16,
            TimeFrame::Days8_32,
            TimeFrame::Days16_64,
        ]
    }
    
    /// Get the range of lookback periods for momentum calculation
    pub fn momentum_range(&self) -> (i32, i32) {
        match self {
            TimeFrame::Minutes15 => (15, 15),
            TimeFrame::Hours1 => (60, 60), 
            TimeFrame::Hours4 => (240, 240),
            TimeFrame::Days1 => (1, 1),
            TimeFrame::Days7 => (7, 7),
            TimeFrame::Days14 => (14, 14),
            TimeFrame::Days2_8 => (2, 8),
            TimeFrame::Days4_16 => (4, 16),
            TimeFrame::Days8_32 => (8, 32),
            TimeFrame::Days16_64 => (16, 64),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnhancedMomentumMetrics {
    pub simple_momentum: f64,
    pub risk_adjusted_momentum: f64,
    pub volatility_normalized_momentum: f64,
    pub momentum_acceleration: f64,
    pub volatility: f64,
    pub sharpe_ratio: f64,
    pub timeframe: TimeFrame,
}

#[derive(Debug, Clone)]
pub struct MultiTimeframeMomentum {
    pub symbol: String,
    pub timeframe_metrics: HashMap<TimeFrame, EnhancedMomentumMetrics>,
    pub composite_score: f64,
    pub weighted_score: f64,
}

#[derive(Debug, Clone)]
pub struct MarketDataUpdate {
    pub symbol: String,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub volume: i64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub symbol: String,
    pub last_price: f64,
    pub bid_price: f64,
    pub ask_price: f64,
    pub volume: i64,
    pub timestamp: DateTime<Utc>,
    pub security_info: Option<SecurityInfo>,
}

#[derive(Debug, Clone)]
pub struct PriceHistory {
    pub symbol: String,
    pub prices: Vec<(DateTime<Utc>, f64)>,
    pub max_size: usize,
}

pub struct MarketDataHandler {
    data: HashMap<i32, MarketData>,
    symbol_map: HashMap<i32, String>,
    price_history: HashMap<String, PriceHistory>,
    security_map: HashMap<String, SecurityInfo>,
}

impl Default for MarketDataHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketDataHandler {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            symbol_map: HashMap::new(),
            price_history: HashMap::new(),
            security_map: HashMap::new(),
        }
    }

    pub fn register_symbol(&mut self, req_id: i32, symbol: String) {
        self.symbol_map.insert(req_id, symbol.clone());
        self.data.insert(
            req_id,
            MarketData {
                symbol: symbol.clone(),
                last_price: 0.0,
                bid_price: 0.0,
                ask_price: 0.0,
                volume: 0,
                timestamp: Utc::now(),
                security_info: self.security_map.get(&symbol).cloned(),
            },
        );
        if !self.price_history.contains_key(&symbol) {
            self.price_history.insert(
                symbol.clone(),
                PriceHistory {
                    symbol,
                    prices: Vec::new(),
                    max_size: 1000,
                },
            );
        }
    }

    pub fn add_historical_price(
        &mut self,
        symbol: &str,
        timestamp: time::OffsetDateTime,
        price: f64,
    ) {
        if let Some(history) = self.price_history.get_mut(symbol) {
            // Convert time::OffsetDateTime to chrono::DateTime<Utc>
            let datetime = DateTime::from_timestamp(timestamp.unix_timestamp(), 0)
                .unwrap_or_else(Utc::now);

            history.prices.push((datetime, price));

            // Keep prices sorted by time
            history.prices.sort_by_key(|(dt, _)| *dt);

            // Trim to max size if needed
            if history.prices.len() > history.max_size {
                let excess = history.prices.len() - history.max_size;
                history.prices.drain(0..excess);
            }
        }
    }

    pub fn update_realtime_data(&mut self, symbol: &str, price: f64, volume: i64) {
        let timestamp = Utc::now();

        // Update current market data
        if let Some(req_id) = self
            .symbol_map
            .iter()
            .find(|(_, s)| s.as_str() == symbol)
            .map(|(id, _)| *id)
        {
            if let Some(data) = self.data.get_mut(&req_id) {
                data.last_price = price;
                data.volume = volume;
                data.timestamp = timestamp;
            }
        }

        // Add to price history
        if let Some(history) = self.price_history.get_mut(symbol) {
            history.prices.push((timestamp, price));
            if history.prices.len() > history.max_size {
                history.prices.remove(0);
            }
        }
    }

    pub fn get_market_data(&self, symbol: &str) -> Option<&MarketData> {
        self.data.values().find(|d| d.symbol == symbol)
    }

    pub fn get_price_history(&self, symbol: &str) -> Option<&PriceHistory> {
        self.price_history.get(symbol)
    }

    pub fn calculate_momentum(&self, symbol: &str, lookback_period: usize) -> Option<f64> {
        let history = self.get_price_history(symbol)?;

        log::debug!(
            "Price history for {}: {} data points",
            symbol,
            history.prices.len()
        );

        if history.prices.len() < lookback_period {
            log::debug!(
                "Insufficient data for {}: {} < {}",
                symbol,
                history.prices.len(),
                lookback_period
            );
            return None;
        }

        let recent_prices = &history.prices[history.prices.len() - lookback_period..];
        let start_price = recent_prices.first()?.1;
        let end_price = recent_prices.last()?.1;

        if start_price > 0.0 {
            let momentum = (end_price - start_price) / start_price;
            log::debug!(
                "Momentum for {}: start={:.4}, end={:.4}, momentum={:.4}",
                symbol,
                start_price,
                end_price,
                momentum
            );
            Some(momentum)
        } else {
            None
        }
    }

    pub fn calculate_enhanced_momentum(
        &self,
        symbol: &str,
        lookback_period: usize,
    ) -> Option<EnhancedMomentumMetrics> {
        let history = self.get_price_history(symbol)?;

        if history.prices.len() < lookback_period + 1 {
            log::debug!(
                "Insufficient data for enhanced momentum {}: {} < {}",
                symbol,
                history.prices.len(),
                lookback_period + 1
            );
            return None;
        }

        let recent_prices = &history.prices[history.prices.len() - lookback_period..];
        let start_price = recent_prices.first()?.1;
        let end_price = recent_prices.last()?.1;

        if start_price <= 0.0 {
            return None;
        }

        // Calculate simple momentum
        let simple_momentum = (end_price - start_price) / start_price;

        // Calculate daily returns for volatility and risk adjustment
        let mut daily_returns = Vec::new();
        for i in 1..recent_prices.len() {
            let prev_price = recent_prices[i - 1].1;
            let curr_price = recent_prices[i].1;
            if prev_price > 0.0 && curr_price > 0.0 {
                let return_val = (curr_price - prev_price) / prev_price;
                // Filter out extreme outliers (>50% moves) which are likely data errors
                if return_val.abs() < 0.5 {
                    daily_returns.push(return_val);
                }
            }
        }

        if daily_returns.is_empty() || daily_returns.len() < 2 {
            return None;
        }

        // Calculate volatility (standard deviation of returns)
        let mean_return = daily_returns.iter().sum::<f64>() / daily_returns.len() as f64;
        let variance = daily_returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / (daily_returns.len() - 1) as f64; // Use sample variance (n-1)
        let volatility = variance.sqrt();

        // Cap volatility at reasonable levels and ensure it's not zero
        let capped_volatility = volatility.clamp(0.0001, 2.0); // Min 0.01%, Max 200%

        // Calculate annualized volatility (assuming 252 trading days)
        let annualized_volatility = capped_volatility * (252.0_f64).sqrt();

        // Calculate Sharpe-like ratio (momentum return per unit of volatility)
        let sharpe_ratio = if capped_volatility > 0.0 {
            simple_momentum / capped_volatility
        } else {
            0.0
        };

        // Risk-adjusted momentum (momentum scaled by inverse volatility)
        let risk_adjusted_momentum = if annualized_volatility > 0.0 {
            simple_momentum / annualized_volatility
        } else {
            simple_momentum
        };

        // Volatility-normalized momentum (standardized by volatility)
        let volatility_normalized_momentum = if capped_volatility > 0.0 {
            simple_momentum / capped_volatility
        } else {
            simple_momentum
        };

        // Calculate momentum acceleration (rate of change of momentum)
        let momentum_acceleration = if lookback_period >= 4 {
            let half_period = lookback_period / 2;
            let first_half = &recent_prices[0..half_period];
            let second_half = &recent_prices[half_period..];

            if let (Some(first_start), Some(first_end)) = (first_half.first(), first_half.last()) {
                if let (Some(second_start), Some(second_end)) =
                    (second_half.first(), second_half.last())
                {
                    let first_momentum = (first_end.1 - first_start.1) / first_start.1;
                    let second_momentum = (second_end.1 - second_start.1) / second_start.1;
                    second_momentum - first_momentum
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };

        log::debug!(
            "Enhanced momentum for {}: simple={:.4}, risk_adj={:.4}, vol_norm={:.4}, accel={:.4}, vol={:.4}, sharpe={:.4}",
            symbol,
            simple_momentum,
            risk_adjusted_momentum,
            volatility_normalized_momentum,
            momentum_acceleration,
            annualized_volatility,
            sharpe_ratio
        );

        Some(EnhancedMomentumMetrics {
            simple_momentum,
            risk_adjusted_momentum,
            volatility_normalized_momentum,
            momentum_acceleration,
            volatility: annualized_volatility,
            sharpe_ratio,
            timeframe: TimeFrame::Days1,
        })
    }
    
    /// Calculate range-based momentum following Carver's approach
    /// This calculates momentum over multiple periods within a range and combines them
    fn calculate_range_based_momentum(
        &self,
        prices: &[(DateTime<Utc>, f64)],
        min_days: i32,
        max_days: i32,
    ) -> f64 {
        if prices.len() < (max_days as usize) + 1 {
            // Fallback to simple start-to-end if insufficient data
            if let (Some(start), Some(end)) = (prices.first(), prices.last()) {
                return (end.1 - start.1) / start.1;
            }
            return 0.0;
        }
        
        let mut momentum_sum = 0.0;
        let mut count = 0;
        
        // Calculate momentum for each period length in the range
        for period_days in min_days..=max_days {
            let period_idx = period_days as usize;
            if period_idx < prices.len() {
                let start_price = prices[prices.len() - period_idx - 1].1;
                let end_price = prices[prices.len() - 1].1;
                
                if start_price > 0.0 {
                    let momentum = (end_price - start_price) / start_price;
                    momentum_sum += momentum;
                    count += 1;
                }
            }
        }
        
        if count > 0 {
            momentum_sum / count as f64
        } else {
            0.0
        }
    }

    pub fn calculate_momentum_for_timeframe(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
    ) -> Option<EnhancedMomentumMetrics> {
        let history = self.get_price_history(symbol)?;
        let now = Utc::now();
        let timeframe_start = now - timeframe.to_duration();

        // Filter prices within the timeframe
        let timeframe_prices: Vec<(DateTime<Utc>, f64)> = history
            .prices
            .iter()
            .filter(|(timestamp, _)| *timestamp >= timeframe_start)
            .cloned()
            .collect();

        if timeframe_prices.len() < 2 {
            log::debug!(
                "Insufficient data for {} momentum {}: {} data points",
                timeframe.label(),
                symbol,
                timeframe_prices.len()
            );
            return None;
        }

        let start_price = timeframe_prices.first()?.1;
        let end_price = timeframe_prices.last()?.1;

        if start_price <= 0.0 {
            return None;
        }

        // Calculate simple momentum based on timeframe type
        let simple_momentum = match timeframe {
            // For Carver's range-based timeframes, calculate momentum over multiple periods
            TimeFrame::Days2_8 | TimeFrame::Days4_16 | TimeFrame::Days8_32 | TimeFrame::Days16_64 => {
                let (min_days, max_days) = timeframe.momentum_range();
                self.calculate_range_based_momentum(&timeframe_prices, min_days, max_days)
            }
            // For traditional timeframes, use simple start-to-end calculation
            _ => (end_price - start_price) / start_price,
        };

        // Calculate returns for volatility and risk adjustment
        let mut returns = Vec::new();
        for i in 1..timeframe_prices.len() {
            let prev_price = timeframe_prices[i - 1].1;
            let curr_price = timeframe_prices[i].1;
            if prev_price > 0.0 && curr_price > 0.0 {
                let return_val = (curr_price - prev_price) / prev_price;
                // Filter out extreme outliers (>50% moves) which are likely data errors
                if return_val.abs() < 0.5 {
                    returns.push(return_val);
                }
            }
        }

        if returns.is_empty() || returns.len() < 2 {
            return None;
        }

        // Calculate volatility (standard deviation of returns)
        let mean_return = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns
            .iter()
            .map(|r| (r - mean_return).powi(2))
            .sum::<f64>()
            / (returns.len() - 1) as f64;
        let volatility = variance.sqrt();

        // Cap volatility at reasonable levels and ensure it's not zero
        let capped_volatility = volatility.clamp(0.0001, 2.0);

        // Scale volatility based on timeframe (annualize it)
        let scaling_factor = match timeframe {
            TimeFrame::Minutes15 => (252.0 * 24.0 * 4.0_f64).sqrt(), // 15-min periods per year
            TimeFrame::Hours1 => (252.0 * 24.0_f64).sqrt(),          // Hours per year
            TimeFrame::Hours4 => (252.0 * 6.0_f64).sqrt(),           // 4-hour periods per year
            TimeFrame::Days1 => (252.0_f64).sqrt(),                  // Days per year
            TimeFrame::Days7 => (52.0_f64).sqrt(),                   // Weeks per year
            TimeFrame::Days14 => (26.0_f64).sqrt(),                  // Bi-weeks per year
            // For Carver's momentum timeframes, use appropriate scaling
            TimeFrame::Days2_8 => (252.0 / 5.0_f64).sqrt(),         // ~5-day periods per year
            TimeFrame::Days4_16 => (252.0 / 10.0_f64).sqrt(),       // ~10-day periods per year
            TimeFrame::Days8_32 => (252.0 / 20.0_f64).sqrt(),       // ~20-day periods per year
            TimeFrame::Days16_64 => (252.0 / 40.0_f64).sqrt(),      // ~40-day periods per year
        };
        let annualized_volatility = capped_volatility * scaling_factor;

        // Calculate Sharpe-like ratio (momentum return per unit of volatility)
        let sharpe_ratio = if capped_volatility > 0.0 {
            simple_momentum / capped_volatility
        } else {
            0.0
        };

        // Risk-adjusted momentum (momentum scaled by inverse volatility)
        let risk_adjusted_momentum = if annualized_volatility > 0.0 {
            simple_momentum / annualized_volatility
        } else {
            simple_momentum
        };

        // Volatility-normalized momentum (standardized by volatility)
        let volatility_normalized_momentum = if capped_volatility > 0.0 {
            simple_momentum / capped_volatility
        } else {
            simple_momentum
        };

        // Calculate momentum acceleration (rate of change of momentum)
        let momentum_acceleration = if timeframe_prices.len() >= 4 {
            let half_point = timeframe_prices.len() / 2;
            let first_half = &timeframe_prices[0..half_point];
            let second_half = &timeframe_prices[half_point..];

            if let (Some(first_start), Some(first_end)) = (first_half.first(), first_half.last()) {
                if let (Some(second_start), Some(second_end)) =
                    (second_half.first(), second_half.last())
                {
                    let first_momentum = (first_end.1 - first_start.1) / first_start.1;
                    let second_momentum = (second_end.1 - second_start.1) / second_start.1;
                    second_momentum - first_momentum
                } else {
                    0.0
                }
            } else {
                0.0
            }
        } else {
            0.0
        };

        log::debug!(
            "{} momentum for {}: simple={:.4}, risk_adj={:.4}, vol_norm={:.4}, accel={:.4}, vol={:.4}, sharpe={:.4}",
            timeframe.label(),
            symbol,
            simple_momentum,
            risk_adjusted_momentum,
            volatility_normalized_momentum,
            momentum_acceleration,
            annualized_volatility,
            sharpe_ratio
        );

        Some(EnhancedMomentumMetrics {
            simple_momentum,
            risk_adjusted_momentum,
            volatility_normalized_momentum,
            momentum_acceleration,
            volatility: annualized_volatility,
            sharpe_ratio,
            timeframe,
        })
    }

    pub fn calculate_multi_timeframe_momentum(
        &self,
        symbol: &str,
    ) -> Option<MultiTimeframeMomentum> {
        let mut timeframe_metrics = HashMap::new();
        let mut available_timeframes = Vec::new();

        // Calculate momentum for all timeframes
        for timeframe in TimeFrame::all_timeframes() {
            if let Some(metrics) = self.calculate_momentum_for_timeframe(symbol, timeframe) {
                timeframe_metrics.insert(timeframe, metrics);
                available_timeframes.push(timeframe);
            }
        }

        if timeframe_metrics.is_empty() {
            return None;
        }

        // Calculate composite score with progressive weighting
        // Shorter timeframes get less weight, longer timeframes get more weight
        let weights = [
            (TimeFrame::Minutes15, 0.05),
            (TimeFrame::Hours1, 0.10),
            (TimeFrame::Hours4, 0.15),
            (TimeFrame::Days1, 0.20),
            (TimeFrame::Days7, 0.25),
            (TimeFrame::Days14, 0.25),
            // Carver's momentum timeframes - higher weights for longer-term signals
            (TimeFrame::Days2_8, 0.15),
            (TimeFrame::Days4_16, 0.20),
            (TimeFrame::Days8_32, 0.25),
            (TimeFrame::Days16_64, 0.30),
        ];

        let mut composite_score = 0.0;
        let mut weight_sum = 0.0;
        let mut weighted_score = 0.0;
        let mut weighted_weight_sum = 0.0;

        for (timeframe, weight) in weights {
            if let Some(metrics) = timeframe_metrics.get(&timeframe) {
                // Use risk-adjusted momentum for composite score
                composite_score += metrics.risk_adjusted_momentum * weight;
                weight_sum += weight;

                // Use simple momentum for weighted score
                weighted_score += metrics.simple_momentum * weight;
                weighted_weight_sum += weight;
            }
        }

        // Normalize scores
        if weight_sum > 0.0 {
            composite_score /= weight_sum;
        }
        if weighted_weight_sum > 0.0 {
            weighted_score /= weighted_weight_sum;
        }

        log::debug!(
            "Multi-timeframe momentum for {}: composite={:.4}, weighted={:.4}, timeframes={:?}",
            symbol,
            composite_score,
            weighted_score,
            available_timeframes
                .iter()
                .map(|tf| tf.label())
                .collect::<Vec<_>>()
        );

        Some(MultiTimeframeMomentum {
            symbol: symbol.to_string(),
            timeframe_metrics,
            composite_score,
            weighted_score,
        })
    }

    pub fn get_latest_prices(&self) -> HashMap<String, f64> {
        let mut prices = HashMap::new();
        for data in self.data.values() {
            if data.last_price > 0.0 {
                prices.insert(data.symbol.to_string(), data.last_price);
            }
        }
        prices
    }

    pub fn register_security(&mut self, symbol: String, security_info: SecurityInfo) {
        self.security_map.insert(symbol, security_info);
    }

    pub fn get_security_info(&self, symbol: &str) -> Option<&SecurityInfo> {
        self.security_map.get(symbol)
    }
}
