use crate::config::{StrategyConfig, RiskConfig};
use crate::bollinger::{BollingerCalculator, BollingerMetrics};
use crate::breakout::{BreakoutCalculator, BreakoutMetrics};
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
    pub breakout_metrics: Option<BreakoutMetrics>,
    pub bollinger_metrics: Option<BollingerMetrics>,
    pub composite_score: f64,
}

pub struct MomentumStrategy {
    config: StrategyConfig,
    current_positions: HashMap<String, f64>,
    volatility_targeter: VolatilityTargeter,
    breakout_calculator: BreakoutCalculator,
    bollinger_calculator: BollingerCalculator,
}

impl MomentumStrategy {
    pub fn new(config: StrategyConfig) -> Self {
        // Initialize volatility targeter with 25% annual target and default risk config
        let risk_config = RiskConfig::default();
        let volatility_targeter = VolatilityTargeter::new(0.25, risk_config);
        let breakout_calculator = BreakoutCalculator::new();
        let bollinger_calculator = BollingerCalculator::new();
        
        Self {
            config,
            current_positions: HashMap::new(),
            volatility_targeter,
            breakout_calculator,
            bollinger_calculator,
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
            
            // Calculate breakout signals
            let breakout_metrics = self.breakout_calculator
                .calculate_multi_timeframe_breakout(&security.symbol, market_data);
            
            // Calculate Bollinger Bands signals
            let bollinger_metrics = self.bollinger_calculator
                .calculate_multi_timeframe_bollinger(&security.symbol, market_data);

            if let Some(momentum) = simple_momentum {
                // Calculate base composite score from momentum
                let momentum_composite = if let Some(ref mtf) = multi_timeframe {
                    mtf.composite_score
                } else if let Some(ref enhanced) = enhanced_metrics {
                    enhanced.risk_adjusted_momentum
                } else {
                    momentum
                };
                
                // Combine momentum, breakout, and Bollinger signals
                let composite_score = {
                    // Scale signals to common range for combination
                    let breakout_signal_scaled = breakout_metrics.as_ref()
                        .map(|b| b.composite_signal / 20.0) // Scale from [-20,+20] to [-1,+1]
                        .unwrap_or(0.0);
                    
                    let bollinger_signal_scaled = bollinger_metrics.as_ref()
                        .map(|b| b.composite_signal / 20.0) // Scale from [-20,+20] to [-1,+1]
                        .unwrap_or(0.0);
                    
                    // Weighted combination: 50% momentum, 30% breakout, 20% bollinger
                    let momentum_weight = 0.5;
                    let breakout_weight = 0.3;
                    let bollinger_weight = 0.2;
                    
                    let combined_signal = momentum_composite * momentum_weight + 
                                        breakout_signal_scaled * breakout_weight +
                                        bollinger_signal_scaled * bollinger_weight;
                    
                    // Apply consensus boost if signals agree
                    let signals = [momentum_composite, breakout_signal_scaled, bollinger_signal_scaled];
                    let positive_signals = signals.iter().filter(|&&s| s > 0.0).count();
                    let negative_signals = signals.iter().filter(|&&s| s < 0.0).count();
                    let total_signals = signals.iter().filter(|&&s| s != 0.0).count();
                    
                    if total_signals > 1 {
                        let consensus_ratio = positive_signals.max(negative_signals) as f64 / total_signals as f64;
                        if consensus_ratio > 0.66 { // 2/3 consensus
                            combined_signal * 1.25 // 25% boost for strong consensus
                        } else {
                            combined_signal
                        }
                    } else {
                        combined_signal
                    }
                };

                momentum_scores.push(MomentumScore {
                    symbol: security.symbol.clone(),
                    momentum,
                    rank: 0,
                    enhanced_metrics: enhanced_metrics.clone(),
                    multi_timeframe: multi_timeframe.clone(),
                    breakout_metrics: breakout_metrics.clone(),
                    bollinger_metrics: bollinger_metrics.clone(),
                    composite_score,
                });

                debug!(
                    "Signals for {}: momentum={:.4}, breakout={:.4}, bollinger={:.4}, composite={:.4}",
                    security.symbol, 
                    momentum_composite,
                    breakout_metrics.as_ref().map_or(0.0, |b| b.composite_signal),
                    bollinger_metrics.as_ref().map_or(0.0, |b| b.composite_signal),
                    composite_score
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
                    debug!("  Multi-timeframe momentum:");
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
                
                if let Some(ref breakout) = breakout_metrics {
                    debug!("  Breakout signals:");
                    debug!("    Composite: {:.4}, Consensus: {:.4}", 
                           breakout.composite_signal, breakout.consensus_strength);
                    for (timeframe, signal) in &breakout.timeframe_signals {
                        debug!(
                            "    {}: type={:?}, strength={:.4}, price={:.4}",
                            timeframe.label(),
                            signal.breakout_type,
                            signal.signal_strength,
                            signal.current_price
                        );
                    }
                }
                
                if let Some(ref bollinger) = bollinger_metrics {
                    debug!("  Bollinger signals:");
                    debug!("    Composite: {:.4}, Volatility regime: {:?}", 
                           bollinger.composite_signal, bollinger.volatility_regime);
                    for (timeframe, signal) in &bollinger.timeframe_signals {
                        debug!(
                            "    {}: type={:?}, strength={:.4}, %B={:.4}, squeeze={}",
                            timeframe.label(),
                            signal.signal_type,
                            signal.signal_strength,
                            signal.bands.percent_b,
                            signal.band_squeeze
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
                s.enhanced_metrics.as_ref().is_none_or(|em| {
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
                        let action = "SELL";
                        let order_type = self.get_order_type();
                        let limit_price = self.calculate_limit_price(action, data.last_price);
                        
                        signals.push(OrderSignal {
                            symbol: position.to_string(),
                            action: action.to_string(),
                            quantity: self.current_positions[position].abs(),
                            price: data.last_price,
                            order_type,
                            limit_price,
                            reason: "Exit position - momentum rank dropped".to_string(),
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
                        let reason = if let Some(ref bollinger) = score.bollinger_metrics {
                            if let Some(ref breakout) = score.breakout_metrics {
                                if let Some(ref enhanced) = score.enhanced_metrics {
                                    format!(
                                        "Triple signal (momentum+breakout+bollinger) - rank: {}, composite: {:.4}, momentum: {:.4}, breakout: {:.4}, bollinger: {:.4}, vol_regime: {:?}",
                                        score.rank,
                                        score.composite_score,
                                        enhanced.risk_adjusted_momentum,
                                        breakout.composite_signal,
                                        bollinger.composite_signal,
                                        bollinger.volatility_regime
                                    )
                                } else {
                                    format!(
                                        "Triple signal (momentum+breakout+bollinger) - rank: {}, composite: {:.4}, momentum: {:.4}, breakout: {:.4}, bollinger: {:.4}",
                                        score.rank,
                                        score.composite_score,
                                        score.momentum,
                                        breakout.composite_signal,
                                        bollinger.composite_signal
                                    )
                                }
                            } else {
                                format!(
                                    "Combined momentum+bollinger signal - rank: {}, composite: {:.4}, momentum: {:.4}, bollinger: {:.4}, vol_regime: {:?}",
                                    score.rank,
                                    score.composite_score,
                                    score.momentum,
                                    bollinger.composite_signal,
                                    bollinger.volatility_regime
                                )
                            }
                        } else if let Some(ref breakout) = score.breakout_metrics {
                            if let Some(ref enhanced) = score.enhanced_metrics {
                                format!(
                                    "Combined momentum+breakout signal - rank: {}, composite: {:.4}, momentum: {:.4}, breakout: {:.4}, consensus: {:.4}",
                                    score.rank,
                                    score.composite_score,
                                    enhanced.risk_adjusted_momentum,
                                    breakout.composite_signal,
                                    breakout.consensus_strength
                                )
                            } else {
                                format!(
                                    "Combined momentum+breakout signal - rank: {}, composite: {:.4}, momentum: {:.4}, breakout: {:.4}",
                                    score.rank,
                                    score.composite_score,
                                    score.momentum,
                                    breakout.composite_signal
                                )
                            }
                        } else if let Some(ref enhanced) = score.enhanced_metrics {
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

                        let order_type = self.get_order_type();
                        let limit_price = self.calculate_limit_price(action, data.last_price);
                        
                        signals.push(OrderSignal {
                            symbol: score.symbol.clone(),
                            action: action.to_string(),
                            quantity: quantity.abs(),
                            price: data.last_price,
                            order_type,
                            limit_price,
                            reason,
                            security_info: security_info.clone(),
                        });
                    }
                }
            }
        }

        signals
    }


    pub fn update_position(&mut self, symbol: &str, quantity: f64) {
        if quantity == 0.0 {
            self.current_positions.remove(symbol);
        } else {
            self.current_positions.insert(symbol.to_string(), quantity);
        }
    }

    /// Calculate limit price based on action and configuration
    fn calculate_limit_price(&self, action: &str, market_price: f64) -> Option<f64> {
        if !self.config.use_limit_orders {
            return None;
        }

        let offset = self.config.limit_order_offset;
        match action {
            "BUY" => {
                // For buy orders, place limit below market price
                Some(market_price * (1.0 - offset))
            }
            "SELL" => {
                // For sell orders, place limit above market price
                Some(market_price * (1.0 + offset))
            }
            _ => None,
        }
    }

    /// Get order type string based on configuration
    fn get_order_type(&self) -> String {
        if self.config.use_limit_orders {
            "LMT".to_string()
        } else {
            "MKT".to_string()
        }
    }

    pub fn get_positions(&self) -> &HashMap<String, f64> {
        &self.current_positions
    }
    
    /// Calculate signal strength following Carver's approach (-20 to +20 scale)
    /// This transforms momentum scores into standardized signal strength
    fn calculate_signal_strength(&self, score: &MomentumScore) -> f64 {
        // Start with the composite score which already incorporates momentum and breakout
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
                (0.3 / enhanced.volatility).clamp(0.5, 1.5)
            } else {
                1.0
            };
            
            sharpe_multiplier * vol_multiplier
        } else {
            1.0
        };
        
        // Step 4: Apply multi-timeframe consensus boost (momentum)
        let momentum_consensus_multiplier = if let Some(ref mtf) = score.multi_timeframe {
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
        
        // Step 5: Apply breakout consensus boost
        let breakout_consensus_multiplier = if let Some(ref breakout) = score.breakout_metrics {
            // Use breakout consensus strength as an additional multiplier
            let consensus_strength = breakout.consensus_strength;
            
            // Strong breakout consensus (>0.7) gets a boost
            if consensus_strength > 0.7 {
                1.25
            } else if consensus_strength > 0.5 {
                1.1
            } else {
                1.0
            }
        } else {
            1.0
        };
        
        // Step 6: Apply Bollinger volatility regime boost
        let bollinger_volatility_multiplier = if let Some(ref bollinger) = score.bollinger_metrics {
            // Use volatility regime as an additional multiplier
            match bollinger.volatility_regime {
                crate::bollinger::VolatilityRegime::High => 1.15, // High volatility = stronger signals
                crate::bollinger::VolatilityRegime::Normal => 1.0, // Normal volatility = baseline
                crate::bollinger::VolatilityRegime::Low => 0.9,   // Low volatility = weaker signals
            }
        } else {
            1.0
        };
        
        // Step 7: Combine all factors and cap at -20 to +20
        let final_signal = scaled_signal * quality_multiplier * momentum_consensus_multiplier * breakout_consensus_multiplier * bollinger_volatility_multiplier;
        let capped_signal = final_signal.clamp(-20.0, 20.0);
        
        debug!(
            "Signal strength calculation for {}: base={:.3}, centered={:.3}, scaled={:.2}, quality_mult={:.2}, momentum_consensus={:.2}, breakout_consensus={:.2}, bollinger_vol={:.2}, final={:.2}",
            score.symbol, base_signal, centered_momentum, scaled_signal, quality_multiplier, momentum_consensus_multiplier, breakout_consensus_multiplier, bollinger_volatility_multiplier, capped_signal
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
                    
                    (raw_position_size * price / contract_value).floor().max(1.0)
                } else {
                    1.0
                }
            }
            SecurityType::Forex => {
                if let Some(ref _forex_pair) = security_info.forex_pair {
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
