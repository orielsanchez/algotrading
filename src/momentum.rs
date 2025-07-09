use crate::config::{StrategyConfig, RiskConfig};
use crate::market_data::{EnhancedMomentumMetrics, MarketDataHandler, MultiTimeframeMomentum};
use crate::orders::OrderSignal;
use crate::security_types::{SecurityInfo, SecurityType};
use crate::volatility::VolatilityTargeter;
use log::{debug, warn};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MomentumScore {
    pub symbol: String,
    pub momentum: f64,
    pub rank: usize,
    pub enhanced_metrics: Option<EnhancedMomentumMetrics>,
    pub multi_timeframe: Option<MultiTimeframeMomentum>,
    pub composite_score: f64,
}

pub struct MomentumStrategy {
    config: StrategyConfig,
    current_positions: HashMap<String, f64>,
    volatility_targeter: VolatilityTargeter,
}

impl MomentumStrategy {
    pub fn new(config: StrategyConfig) -> Self {
        // Initialize volatility targeter with 25% annual target and default risk config
        let risk_config = RiskConfig::default();
        let volatility_targeter = VolatilityTargeter::new(0.25, risk_config);
        
        Self {
            config,
            current_positions: HashMap::new(),
            volatility_targeter,
        }
    }

    pub fn calculate_signals(&mut self, market_data: &MarketDataHandler) -> Vec<OrderSignal> {
        // Update volatility data with current prices
        let mut current_prices = HashMap::new();
        for security in &self.config.securities {
            if let Some(market_data_point) = market_data.get_market_data(&security.symbol) {
                current_prices.insert(security.symbol.clone(), market_data_point.last_price);
            }
        }
        self.volatility_targeter.update_prices(&current_prices);
        
        let mut momentum_scores: Vec<MomentumScore> = Vec::new();

        for security in &self.config.securities {
            // Calculate both simple and enhanced momentum
            let simple_momentum =
                market_data.calculate_momentum(&security.symbol, self.config.lookback_period);
            let enhanced_metrics = market_data
                .calculate_enhanced_momentum(&security.symbol, self.config.lookback_period);
            let multi_timeframe = market_data.calculate_multi_timeframe_momentum(&security.symbol);

            if let Some(momentum) = simple_momentum {
                // Use composite score from multi-timeframe analysis if available, otherwise use risk-adjusted momentum
                let composite_score = if let Some(ref mtf) = multi_timeframe {
                    mtf.composite_score
                } else if let Some(ref enhanced) = enhanced_metrics {
                    enhanced.risk_adjusted_momentum
                } else {
                    momentum
                };

                momentum_scores.push(MomentumScore {
                    symbol: security.symbol.clone(),
                    momentum,
                    rank: 0,
                    enhanced_metrics: enhanced_metrics.clone(),
                    multi_timeframe: multi_timeframe.clone(),
                    composite_score,
                });

                debug!(
                    "Momentum for {}: simple={:.4}, composite={:.4}",
                    security.symbol, momentum, composite_score
                );
                if let Some(ref enhanced) = enhanced_metrics {
                    debug!(
                        "  Enhanced metrics - risk_adj={:.4}, vol_norm={:.4}, accel={:.4}, vol={:.4}, sharpe={:.4}",
                        enhanced.risk_adjusted_momentum,
                        enhanced.volatility_normalized_momentum,
                        enhanced.momentum_acceleration,
                        enhanced.volatility,
                        enhanced.sharpe_ratio
                    );
                }

                if let Some(ref mtf) = multi_timeframe {
                    debug!("  Multi-timeframe metrics:");
                    for (timeframe, metrics) in &mtf.timeframe_metrics {
                        debug!(
                            "    {}: simple={:.4}, risk_adj={:.4}, vol={:.4}",
                            timeframe.label(),
                            metrics.simple_momentum,
                            metrics.risk_adjusted_momentum,
                            metrics.volatility
                        );
                    }
                }
            } else {
                debug!(
                    "No momentum calculated for {} (insufficient data?)",
                    security.symbol
                );
            }
        }

        // Sort by composite score instead of simple momentum
        momentum_scores.sort_by(|a, b| b.composite_score.partial_cmp(&a.composite_score).unwrap());

        for (i, score) in momentum_scores.iter_mut().enumerate() {
            score.rank = i + 1;
        }

        // Filter based on composite score and enhanced criteria
        let top_performers: Vec<&MomentumScore> = momentum_scores
            .iter()
            .filter(|s| {
                // Basic momentum threshold
                s.composite_score > self.config.momentum_threshold &&
                // Additional quality filters
                s.enhanced_metrics.as_ref().map_or(true, |em| {
                    // Filter out high volatility stocks (risk management)
                    em.volatility < 0.5 && // Less than 50% annualized volatility
                    // Ensure momentum has some consistency (positive Sharpe-like ratio)
                    em.sharpe_ratio > 0.1
                })
            })
            .take(5)
            .collect();

        debug!(
            "Top momentum securities: {:?}",
            top_performers.iter().map(|s| &s.symbol).collect::<Vec<_>>()
        );
        for performer in &top_performers {
            debug!(
                "  {}: composite={:.4}, simple={:.4}, rank={}",
                performer.symbol, performer.composite_score, performer.momentum, performer.rank
            );
        }

        let mut signals = Vec::new();

        for position in self.current_positions.keys() {
            if !top_performers.iter().any(|s| &s.symbol == position) {
                if let Some(data) = market_data.get_market_data(position) {
                    if let Some(security_info) = market_data.get_security_info(position) {
                        signals.push(OrderSignal {
                            symbol: position.to_string(),
                            action: "SELL".to_string(),
                            quantity: self.current_positions[position].abs(),
                            price: data.last_price,
                            order_type: "MKT".to_string(),
                            reason: format!("Exit position - momentum rank dropped"),
                            security_info: security_info.clone(),
                        });
                    }
                }
            }
        }

        for score in top_performers {
            if let Some(data) = market_data.get_market_data(&score.symbol) {
                if let Some(security_info) = market_data.get_security_info(&score.symbol) {
                    // Convert momentum score to signal strength in Carver's -20 to +20 scale
                    let signal_strength = self.calculate_signal_strength(score);
                    
                    // Use a default portfolio value of $100,000 for now
                    // TODO: This should come from the portfolio manager
                    let portfolio_value = 100_000.0;
                    
                    let target_position = self.calculate_volatility_based_position_size(
                        &score.symbol,
                        signal_strength,
                        security_info,
                        data.last_price,
                        portfolio_value,
                    );
                    let current_position = self
                        .current_positions
                        .get(&score.symbol)
                        .copied()
                        .unwrap_or(0.0);

                    debug!(
                        "Position sizing for {}: target={:.0}, current={:.0}, diff={:.0}",
                        score.symbol,
                        target_position,
                        current_position,
                        target_position - current_position
                    );

                    if (target_position - current_position).abs() > 0.01 {
                        let quantity = target_position - current_position;
                        let reason = if let Some(ref enhanced) = score.enhanced_metrics {
                            format!(
                                "Enhanced momentum signal - rank: {}, composite: {:.4}, risk_adj: {:.4}, sharpe: {:.4}",
                                score.rank,
                                score.composite_score,
                                enhanced.risk_adjusted_momentum,
                                enhanced.sharpe_ratio
                            )
                        } else {
                            format!(
                                "Momentum signal - rank: {}, momentum: {:.4}",
                                score.rank, score.momentum
                            )
                        };

                        let action = if quantity > 0.0 { "BUY" } else { "SELL" };
                        
                        // Log forex-specific trade interpretation
                        if let Some(ref forex_pair) = security_info.forex_pair {
                            debug!(
                                "Forex signal: {} {} units of {} (pair: {}) - {} {} with {}",
                                action,
                                quantity.abs(),
                                forex_pair.base_currency,
                                forex_pair.pair_symbol,
                                if quantity > 0.0 { "Buying" } else { "Selling" },
                                forex_pair.base_currency,
                                forex_pair.quote_currency
                            );
                        }

                        signals.push(OrderSignal {
                            symbol: score.symbol.clone(),
                            action: action.to_string(),
                            quantity: quantity.abs(),
                            price: data.last_price,
                            order_type: "MKT".to_string(),
                            reason,
                            security_info: security_info.clone(),
                        });
                    }
                }
            }
        }

        signals
    }

    fn calculate_position_size(
        &self,
        _symbol: &str,
        momentum: f64,
        security_info: &SecurityInfo,
        price: f64,
    ) -> f64 {
        let base_size = self.config.position_size;
        let momentum_multiplier = 1.0 + (momentum - self.config.momentum_threshold);
        let adjusted_size = base_size * momentum_multiplier.min(2.0).max(0.5);

        match security_info.security_type {
            SecurityType::Stock => adjusted_size / price,
            SecurityType::Future => {
                if let Some(specs) = &security_info.contract_specs {
                    let contract_value = price * specs.multiplier;
                    (adjusted_size / contract_value).floor()
                } else {
                    1.0
                }
            }
            SecurityType::Forex => {
                // For forex, calculate position size in base currency units
                // adjusted_size is the dollar amount we want to risk
                // price is the exchange rate (quote currency per base currency)
                // Result should be base currency units
                
                if let Some(ref _forex_pair) = security_info.forex_pair {
                    // Calculate base currency units needed
                    let base_currency_units = adjusted_size / price;
                    
                    // Round to appropriate lot size
                    // Standard lot = 100,000, mini lot = 10,000, micro lot = 1,000
                    let lot_size = if base_currency_units >= 100_000.0 {
                        100_000.0  // Standard lot
                    } else if base_currency_units >= 10_000.0 {
                        10_000.0   // Mini lot
                    } else {
                        1_000.0    // Micro lot
                    };
                    
                    (base_currency_units / lot_size).floor() * lot_size
                } else {
                    // Fallback for old format
                    (adjusted_size / 1000.0).floor() * 1000.0
                }
            }
        }
    }

    fn calculate_enhanced_position_size(
        &self,
        symbol: &str,
        score: &MomentumScore,
        security_info: &SecurityInfo,
        price: f64,
    ) -> f64 {
        let base_size = self.config.position_size;

        // TODO: This should be updated to use portfolio-based sizing
        // For now, keeping the existing logic but this needs RiskManager integration

        // Use enhanced metrics if available for better position sizing
        let (momentum_multiplier, volatility_adjustment) =
            if let Some(ref enhanced) = score.enhanced_metrics {
                // Use risk-adjusted momentum for sizing
                let momentum_mult =
                    1.0 + (enhanced.risk_adjusted_momentum - self.config.momentum_threshold * 0.5);

                // Adjust for volatility - reduce size for high volatility stocks
                let vol_adj = if enhanced.volatility > 0.0 {
                    // Scale down position size for high volatility (inverse relationship)
                    // Volatility of 0.2 (20%) = 1.0x, 0.4 (40%) = 0.5x, 0.6 (60%) = 0.33x
                    (0.2 / enhanced.volatility).min(2.0).max(0.25)
                } else {
                    1.0
                };

                (momentum_mult, vol_adj)
            } else {
                // Fallback to simple momentum
                let momentum_mult = 1.0 + (score.momentum - self.config.momentum_threshold);
                (momentum_mult, 1.0)
            };

        // Additional boost for multi-timeframe confirmation
        let timeframe_boost = if let Some(ref mtf) = score.multi_timeframe {
            // Calculate boost based on timeframe consensus
            let positive_timeframes = mtf
                .timeframe_metrics
                .values()
                .filter(|metrics| metrics.risk_adjusted_momentum > 0.0)
                .count();

            let total_timeframes = mtf.timeframe_metrics.len();

            if total_timeframes > 0 {
                let consensus_ratio = positive_timeframes as f64 / total_timeframes as f64;
                // Boost ranges from 0.8x (all negative) to 1.2x (all positive)
                0.8 + (consensus_ratio * 0.4)
            } else {
                1.0
            }
        } else {
            1.0
        };

        let adjusted_size = base_size
            * momentum_multiplier.min(2.0).max(0.5)
            * volatility_adjustment
            * timeframe_boost;

        debug!(
            "Enhanced position sizing for {}: base={:.0}, momentum_mult={:.2}, vol_adj={:.2}, timeframe_boost={:.2}, final_size={:.0}",
            symbol,
            base_size,
            momentum_multiplier,
            volatility_adjustment,
            timeframe_boost,
            adjusted_size
        );

        match security_info.security_type {
            SecurityType::Stock => adjusted_size / price,
            SecurityType::Future => {
                if let Some(specs) = &security_info.contract_specs {
                    let contract_value = price * specs.multiplier;
                    (adjusted_size / contract_value).floor().max(1.0)
                } else {
                    1.0
                }
            }
            SecurityType::Forex => {
                if let Some(ref forex_pair) = security_info.forex_pair {
                    // Debug logging for forex position sizing
                    debug!("Forex position sizing for {}: adjusted_size={}, price={}, pair={:?}", 
                           symbol, adjusted_size, price, forex_pair);
                    
                    // Calculate base currency units needed
                    let base_currency_units = adjusted_size / price;
                    
                    // Sanity check for extreme position sizes
                    if base_currency_units > 1_000_000.0 {
                        warn!("Extremely large forex position calculated for {}: {} units at price {}. May indicate price data issue.", 
                              symbol, base_currency_units, price);
                        return 1_000.0; // Return minimum micro lot
                    }
                    
                    // Round to appropriate lot size
                    let lot_size = if base_currency_units >= 100_000.0 {
                        100_000.0  // Standard lot
                    } else if base_currency_units >= 10_000.0 {
                        10_000.0   // Mini lot
                    } else {
                        1_000.0    // Micro lot
                    };
                    
                    let final_size = (base_currency_units / lot_size).floor().max(1.0) * lot_size;
                    debug!("Final forex position size for {}: {} units (lot_size={})", 
                           symbol, final_size, lot_size);
                    
                    final_size
                } else {
                    // Fallback for old format
                    ((adjusted_size / 1000.0).floor() * 1000.0).max(1000.0)
                }
            }
        }
    }

    pub fn update_position(&mut self, symbol: &str, quantity: f64) {
        if quantity == 0.0 {
            self.current_positions.remove(symbol);
        } else {
            self.current_positions.insert(symbol.to_string(), quantity);
        }
    }

    pub fn get_positions(&self) -> &HashMap<String, f64> {
        &self.current_positions
    }
    
    /// Calculate signal strength following Carver's approach (-20 to +20 scale)
    /// This transforms momentum scores into standardized signal strength
    fn calculate_signal_strength(&self, score: &MomentumScore) -> f64 {
        // Start with the composite score which already incorporates multiple factors
        let base_signal = score.composite_score;
        
        // Apply Carver's signal strength scaling
        // In Carver's framework, signals are typically normalized to have a reasonable range
        // with most signals falling within -20 to +20
        
        // Step 1: Convert momentum to z-score-like metric (centered around 0)
        let centered_momentum = base_signal - self.config.momentum_threshold;
        
        // Step 2: Scale to appropriate range
        // Assuming momentum typically ranges from -0.5 to +0.5 after centering
        // Scale to get most signals in -10 to +10 range, with occasional stronger signals
        let scaled_signal = centered_momentum * 20.0;
        
        // Step 3: Apply additional scaling factors based on signal quality
        let quality_multiplier = if let Some(ref enhanced) = score.enhanced_metrics {
            // Use Sharpe ratio as a quality indicator
            let sharpe_multiplier = if enhanced.sharpe_ratio > 0.5 {
                1.2  // Boost high-quality signals
            } else if enhanced.sharpe_ratio < 0.1 {
                0.8  // Reduce low-quality signals
            } else {
                1.0
            };
            
            // Use volatility as another quality factor
            let vol_multiplier = if enhanced.volatility > 0.0 {
                // Moderate volatility adjustment - don't completely kill high-vol signals
                // but reduce them somewhat
                (0.3 / enhanced.volatility).min(1.5).max(0.5)
            } else {
                1.0
            };
            
            sharpe_multiplier * vol_multiplier
        } else {
            1.0
        };
        
        // Step 4: Apply multi-timeframe consensus boost
        let consensus_multiplier = if let Some(ref mtf) = score.multi_timeframe {
            // Calculate the strength of multi-timeframe consensus
            let positive_signals = mtf.timeframe_metrics.values()
                .filter(|m| m.risk_adjusted_momentum > 0.0)
                .count() as f64;
            let total_signals = mtf.timeframe_metrics.len() as f64;
            
            if total_signals > 0.0 {
                let consensus_ratio = positive_signals / total_signals;
                // Strong consensus (>75%) gets a boost, weak consensus (<25%) gets reduced
                if consensus_ratio > 0.75 {
                    1.3
                } else if consensus_ratio < 0.25 {
                    0.7
                } else {
                    1.0
                }
            } else {
                1.0
            }
        } else {
            1.0
        };
        
        // Step 5: Combine all factors and cap at -20 to +20
        let final_signal = scaled_signal * quality_multiplier * consensus_multiplier;
        let capped_signal = final_signal.min(20.0).max(-20.0);
        
        debug!(
            "Signal strength calculation for {}: base={:.3}, centered={:.3}, scaled={:.2}, quality_mult={:.2}, consensus_mult={:.2}, final={:.2}",
            score.symbol, base_signal, centered_momentum, scaled_signal, quality_multiplier, consensus_multiplier, capped_signal
        );
        
        capped_signal
    }
    
    /// Calculate position size using Carver's volatility targeting approach
    fn calculate_volatility_based_position_size(
        &self,
        symbol: &str,
        signal_strength: f64,
        security_info: &SecurityInfo,
        price: f64,
        portfolio_value: f64,
    ) -> f64 {
        // Use volatility targeter for position sizing
        let raw_position_size = self.volatility_targeter.calculate_position_size(
            symbol,
            signal_strength,
            portfolio_value,
            price,
        );
        
        // Apply security-specific adjustments
        let adjusted_size = match security_info.security_type {
            SecurityType::Stock => raw_position_size.round(),
            SecurityType::Future => {
                if let Some(specs) = &security_info.contract_specs {
                    let contract_value = price * specs.multiplier;
                    let contracts = (raw_position_size * price / contract_value).floor().max(1.0);
                    contracts
                } else {
                    1.0
                }
            }
            SecurityType::Forex => {
                if let Some(ref forex_pair) = security_info.forex_pair {
                    // For forex, raw_position_size is already in dollar terms
                    let base_currency_units = raw_position_size / price;
                    
                    // Sanity check for extreme position sizes
                    if base_currency_units > 1_000_000.0 {
                        warn!("Extremely large forex position calculated for {}: {} units at price {}. May indicate price data issue.", 
                              symbol, base_currency_units, price);
                        return 1_000.0; // Return minimum micro lot
                    }
                    
                    // Round to appropriate lot size
                    let lot_size = if base_currency_units >= 100_000.0 {
                        100_000.0  // Standard lot
                    } else if base_currency_units >= 10_000.0 {
                        10_000.0   // Mini lot
                    } else {
                        1_000.0    // Micro lot
                    };
                    
                    let final_size = (base_currency_units / lot_size).floor().max(1.0) * lot_size;
                    debug!("Volatility-based forex position size for {}: {} units (lot_size={}, signal_strength={:.2})", 
                           symbol, final_size, lot_size, signal_strength);
                    
                    final_size
                } else {
                    // Fallback for old format
                    ((raw_position_size / 1000.0).floor() * 1000.0).max(1000.0)
                }
            }
        };
        
        debug!(
            "Volatility-based position sizing for {}: signal_strength={:.2}, raw_size={:.0}, adjusted_size={:.0}, price={:.4}",
            symbol, signal_strength, raw_position_size, adjusted_size, price
        );
        
        adjusted_size
    }
}
