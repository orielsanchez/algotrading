use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::security_types::{SecurityInfo, SecurityType};

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub average_cost: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub security_info: Option<SecurityInfo>,
}

#[derive(Debug, Clone)]
pub struct PortfolioStats {
    pub total_value: f64,
    pub cash_balance: f64,
    pub total_unrealized_pnl: f64,
    pub total_realized_pnl: f64,
    pub positions_count: usize,
    pub timestamp: DateTime<Utc>,
}

pub struct Portfolio {
    positions: HashMap<String, Position>,
    cash_balance: f64,
    initial_cash: f64,
    total_realized_pnl: f64,
    security_map: HashMap<String, SecurityInfo>,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            positions: HashMap::new(),
            cash_balance: initial_cash,
            initial_cash,
            total_realized_pnl: 0.0,
            security_map: HashMap::new(),
        }
    }
    
    pub fn register_security(&mut self, symbol: String, security_info: SecurityInfo) {
        self.security_map.insert(symbol, security_info);
    }
    
    pub fn update_position(&mut self, symbol: String, quantity: f64, price: f64) {
        if let Some(position) = self.positions.get_mut(&symbol) {
            if quantity > 0.0 {
                let total_cost = position.average_cost * position.quantity + price * quantity;
                position.quantity += quantity;
                position.average_cost = total_cost / position.quantity;
            } else {
                let sell_quantity = quantity.abs();
                let realized_pnl = sell_quantity * (price - position.average_cost);
                position.realized_pnl += realized_pnl;
                self.total_realized_pnl += realized_pnl;
                position.quantity -= sell_quantity;
                
                if position.quantity <= 0.0 {
                    self.positions.remove(&symbol);
                }
            }
        } else if quantity > 0.0 {
            self.positions.insert(symbol.clone(), Position {
                symbol: symbol.clone(),
                quantity,
                average_cost: price,
                current_price: price,
                unrealized_pnl: 0.0,
                realized_pnl: 0.0,
                security_info: self.security_map.get(&symbol).cloned(),
            });
        }
        
        let trade_value = if let Some(security_info) = self.security_map.get(&symbol) {
            security_info.get_position_value(price, quantity)
        } else {
            quantity * price
        };
        self.cash_balance -= trade_value;
    }
    
    pub fn update_market_prices(&mut self, prices: &HashMap<String, f64>) {
        for (symbol, position) in &mut self.positions {
            if let Some(&price) = prices.get(symbol) {
                position.current_price = price;
                
                let pnl_per_unit = price - position.average_cost;
                position.unrealized_pnl = if let Some(security_info) = &position.security_info {
                    match security_info.security_type {
                        SecurityType::Stock => position.quantity * pnl_per_unit,
                        SecurityType::Future => {
                            if let Some(specs) = &security_info.contract_specs {
                                position.quantity * pnl_per_unit * specs.multiplier
                            } else {
                                position.quantity * pnl_per_unit
                            }
                        }
                    }
                } else {
                    position.quantity * pnl_per_unit
                };
            }
        }
    }
    
    pub fn get_stats(&self) -> PortfolioStats {
        let total_position_value: f64 = self.positions
            .values()
            .map(|p| {
                if let Some(security_info) = &p.security_info {
                    security_info.get_position_value(p.current_price, p.quantity)
                } else {
                    p.quantity * p.current_price
                }
            })
            .sum();
        
        let total_unrealized_pnl: f64 = self.positions
            .values()
            .map(|p| p.unrealized_pnl)
            .sum();
        
        PortfolioStats {
            total_value: self.cash_balance + total_position_value,
            cash_balance: self.cash_balance,
            total_unrealized_pnl,
            total_realized_pnl: self.total_realized_pnl,
            positions_count: self.positions.len(),
            timestamp: Utc::now(),
        }
    }
    
    pub fn get_position(&self, symbol: &str) -> Option<&Position> {
        self.positions.get(symbol)
    }
    
    pub fn get_all_positions(&self) -> &HashMap<String, Position> {
        &self.positions
    }
}