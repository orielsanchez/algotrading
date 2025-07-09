use crate::connection::AccountPosition;
use crate::security_types::{SecurityInfo, SecurityType};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub average_cost: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
    pub security_info: Option<SecurityInfo>,
    // Margin fields
    pub initial_margin: f64,
    pub maintenance_margin: f64,
    pub margin_utilization: f64,
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
    total_realized_pnl: f64,
    security_map: HashMap<String, SecurityInfo>,
    // Margin tracking
    pub total_initial_margin: f64,
    pub total_maintenance_margin: f64,
    pub excess_liquidity: f64,
    pub margin_cushion: f64,
}

impl Portfolio {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            positions: HashMap::new(),
            cash_balance: initial_cash,
            total_realized_pnl: 0.0,
            security_map: HashMap::new(),
            total_initial_margin: 0.0,
            total_maintenance_margin: 0.0,
            excess_liquidity: initial_cash,
            margin_cushion: 1.0,
        }
    }

    pub fn register_security(&mut self, symbol: String, security_info: SecurityInfo) {
        self.security_map.insert(symbol, security_info);
    }

    pub fn update_position(&mut self, symbol: &str, quantity: f64, price: f64) {
        if let Some(position) = self.positions.get_mut(symbol) {
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
                    self.positions.remove(symbol);
                }
            }
        } else if quantity > 0.0 {
            self.positions.insert(
                symbol.to_string(),
                Position {
                    symbol: symbol.to_string(),
                    quantity,
                    average_cost: price,
                    current_price: price,
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    security_info: self.security_map.get(symbol).cloned(),
                    initial_margin: 0.0,
                    maintenance_margin: 0.0,
                    margin_utilization: 0.0,
                },
            );
        }

        let trade_value = if let Some(security_info) = self.security_map.get(symbol) {
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
                        SecurityType::Forex => position.quantity * pnl_per_unit,
                    }
                } else {
                    position.quantity * pnl_per_unit
                };
            }
        }
    }

    pub fn get_stats(&self) -> PortfolioStats {
        let total_position_value: f64 = self
            .positions
            .values()
            .map(|p| {
                if let Some(security_info) = &p.security_info {
                    security_info.get_position_value(p.current_price, p.quantity)
                } else {
                    p.quantity * p.current_price
                }
            })
            .sum();

        let total_unrealized_pnl: f64 = self.positions.values().map(|p| p.unrealized_pnl).sum();

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

    /// Update position with margin information
    pub fn update_position_margin(
        &mut self,
        symbol: &str,
        initial_margin: f64,
        maintenance_margin: f64,
    ) {
        if let Some(position) = self.positions.get_mut(symbol) {
            position.initial_margin = initial_margin;
            position.maintenance_margin = maintenance_margin;

            // Calculate margin utilization for this position
            if self.excess_liquidity > 0.0 {
                position.margin_utilization = initial_margin / self.excess_liquidity;
            }
        }
    }

    /// Recalculate total portfolio margin requirements
    pub fn recalculate_margin_totals(&mut self) {
        self.total_initial_margin = self.positions.values().map(|p| p.initial_margin).sum();

        self.total_maintenance_margin = self.positions.values().map(|p| p.maintenance_margin).sum();
    }

    /// Update portfolio margin statistics from account data
    pub fn update_margin_stats(&mut self, account_summary: &HashMap<String, f64>) {
        if let Some(&excess_liq) = account_summary.get("ExcessLiquidity") {
            self.excess_liquidity = excess_liq;
        }

        if let Some(&net_liq) = account_summary.get("NetLiquidation") {
            if net_liq > 0.0 && self.total_maintenance_margin > 0.0 {
                self.margin_cushion = (net_liq - self.total_maintenance_margin) / net_liq;
            }
        }
    }

    /// Get positions as reference for margin calculations
    pub fn positions(&self) -> &HashMap<String, Position> {
        &self.positions
    }

    /// Update cash balance without destroying existing positions
    pub fn update_cash_balance(&mut self, cash: f64) {
        self.cash_balance = cash;
    }

    /// Sync position from TWS API data
    pub fn sync_position_from_tws(&mut self, tws_pos: &AccountPosition, current_price: f64) {
        let symbol = &tws_pos.symbol;
        let quantity = tws_pos.position;
        let avg_cost = tws_pos.avg_cost;

        // Get security info if available
        let security_info = self.security_map.get(symbol).cloned();

        // Calculate unrealized P&L
        let pnl_per_unit = current_price - avg_cost;
        let unrealized_pnl = if let Some(ref security_info) = security_info {
            match security_info.security_type {
                SecurityType::Stock => quantity * pnl_per_unit,
                SecurityType::Future => {
                    if let Some(specs) = &security_info.contract_specs {
                        quantity * pnl_per_unit * specs.multiplier
                    } else {
                        quantity * pnl_per_unit
                    }
                }
                SecurityType::Forex => quantity * pnl_per_unit,
            }
        } else {
            quantity * pnl_per_unit
        };

        // Update or create position
        if quantity != 0.0 {
            let position = Position {
                symbol: symbol.to_string(),
                quantity,
                average_cost: avg_cost,
                current_price,
                unrealized_pnl,
                realized_pnl: 0.0, // We don't have realized P&L from TWS positions
                security_info,
                initial_margin: 0.0,
                maintenance_margin: 0.0,
                margin_utilization: 0.0,
            };

            self.positions.insert(symbol.to_string(), position);
        } else {
            // Remove position if quantity is 0
            self.positions.remove(symbol);
        }
    }

    /// Sync all positions from TWS with current market prices
    pub fn sync_all_positions_from_tws(
        &mut self,
        tws_positions: &[AccountPosition],
        market_prices: &HashMap<String, f64>,
    ) {
        // Clear existing positions since we're doing a full sync
        self.positions.clear();

        for tws_pos in tws_positions {
            // Get current price from market data, fallback to average cost
            let current_price = market_prices
                .get(&tws_pos.symbol)
                .copied()
                .unwrap_or(tws_pos.avg_cost);

            self.sync_position_from_tws(tws_pos, current_price);
        }
    }
}
