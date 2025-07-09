use crate::config::StrategyConfig;
use crate::market_data::MarketDataHandler;
use crate::orders::OrderSignal;
use crate::security_types::{SecurityInfo, SecurityType};
use log::{info, debug};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MomentumScore {
    pub symbol: String,
    pub momentum: f64,
    pub rank: usize,
}

pub struct MomentumStrategy {
    config: StrategyConfig,
    current_positions: HashMap<String, f64>,
}

impl MomentumStrategy {
    pub fn new(config: StrategyConfig) -> Self {
        Self {
            config,
            current_positions: HashMap::new(),
        }
    }
    
    pub fn calculate_signals(&mut self, market_data: &MarketDataHandler) -> Vec<OrderSignal> {
        let mut momentum_scores: Vec<MomentumScore> = Vec::new();
        
        for security in &self.config.securities {
            if let Some(momentum) = market_data.calculate_momentum(&security.symbol, self.config.lookback_period) {
                momentum_scores.push(MomentumScore {
                    symbol: security.symbol.clone(),
                    momentum,
                    rank: 0,
                });
                debug!("Momentum for {}: {:.4}", security.symbol, momentum);
            }
        }
        
        momentum_scores.sort_by(|a, b| b.momentum.partial_cmp(&a.momentum).unwrap());
        
        for (i, score) in momentum_scores.iter_mut().enumerate() {
            score.rank = i + 1;
        }
        
        let top_performers: Vec<&MomentumScore> = momentum_scores
            .iter()
            .filter(|s| s.momentum > self.config.momentum_threshold)
            .take(5)
            .collect();
        
        info!("Top momentum securities: {:?}", top_performers.iter().map(|s| &s.symbol).collect::<Vec<_>>());
        
        let mut signals = Vec::new();
        
        for position in self.current_positions.keys() {
            if !top_performers.iter().any(|s| &s.symbol == position) {
                if let Some(data) = market_data.get_market_data(position) {
                    if let Some(security_info) = market_data.get_security_info(position) {
                        signals.push(OrderSignal {
                            symbol: position.clone(),
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
                    let target_position = self.calculate_position_size(&score.symbol, score.momentum, security_info, data.last_price);
                    let current_position = self.current_positions.get(&score.symbol).copied().unwrap_or(0.0);
                    
                    if (target_position - current_position).abs() > 0.01 {
                        let quantity = target_position - current_position;
                        signals.push(OrderSignal {
                            symbol: score.symbol.clone(),
                            action: if quantity > 0.0 { "BUY".to_string() } else { "SELL".to_string() },
                            quantity: quantity.abs(),
                            price: data.last_price,
                            order_type: "MKT".to_string(),
                            reason: format!("Momentum signal - rank: {}, momentum: {:.4}", score.rank, score.momentum),
                            security_info: security_info.clone(),
                        });
                    }
                }
            }
        }
        
        signals
    }
    
    fn calculate_position_size(&self, _symbol: &str, momentum: f64, security_info: &SecurityInfo, price: f64) -> f64 {
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
        }
    }
    
    pub fn update_position(&mut self, symbol: String, quantity: f64) {
        if quantity == 0.0 {
            self.current_positions.remove(&symbol);
        } else {
            self.current_positions.insert(symbol, quantity);
        }
    }
    
    pub fn get_positions(&self) -> &HashMap<String, f64> {
        &self.current_positions
    }
}