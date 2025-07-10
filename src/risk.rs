use crate::config::RiskConfig;
use crate::portfolio::{Portfolio, Position};
use crate::security_types::SecurityType;
use anyhow::Result;
use log::{error, info, warn};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RiskMetrics {
    pub portfolio_value: f64,
    pub max_position_value: f64,
    pub current_exposure: f64,
    pub available_capital: f64,
    pub risk_budget_used: f64,
    pub positions_at_risk: usize,
}

#[derive(Debug, Clone)]
pub struct PositionRisk {
    pub symbol: String,
    pub current_value: f64,
    pub percentage_of_portfolio: f64,
    pub stop_loss_price: f64,
    pub take_profit_price: f64,
    pub risk_amount: f64,
    pub max_loss_percentage: f64,
    pub needs_stop_loss: bool,
    pub needs_take_profit: bool,
    pub exceeds_position_limit: bool,
}

#[derive(Debug, Clone)]
pub struct RiskSignal {
    pub symbol: String,
    pub action: RiskAction,
    pub quantity: f64,
    pub reason: String,
    pub urgency: RiskUrgency,
}

#[derive(Debug, Clone)]
pub enum RiskAction {
    ReducePosition,
    AddStopLoss,
    AddTakeProfit,
    ClosePosition,
    HoldPosition,
}

#[derive(Debug, Clone)]
pub enum RiskUrgency {
    Critical, // Immediate action required
    High,     // Action required within minutes
    Medium,   // Action required within hours
    Low,      // Monitor closely
}

pub struct RiskManager {
    pub config: RiskConfig,
    stop_losses: HashMap<String, f64>,
    take_profits: HashMap<String, f64>,
}

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            stop_losses: HashMap::new(),
            take_profits: HashMap::new(),
        }
    }

    /// Calculate maximum position size based on portfolio value and risk percentage
    pub fn calculate_max_position_size(
        &self,
        portfolio: &Portfolio,
        symbol: &str,
        price: f64,
    ) -> f64 {
        let portfolio_stats = portfolio.get_stats();
        let portfolio_value = portfolio_stats.total_value;

        // Use configured position size percentage (default 1% of portfolio)
        let max_position_value = portfolio_value * (self.config.max_position_size / 100.0);

        // Calculate position size based on security type
        if let Some(position) = portfolio.get_position(symbol) {
            if let Some(ref security_info) = position.security_info {
                match security_info.security_type {
                    SecurityType::Stock => max_position_value / price,
                    SecurityType::Future => {
                        if let Some(specs) = &security_info.contract_specs {
                            let contract_value = price * specs.multiplier;
                            (max_position_value / contract_value).floor()
                        } else {
                            1.0
                        }
                    }
                    SecurityType::Forex => {
                        // For forex, position size is in base currency units
                        ((max_position_value / price) / 1000.0).floor() * 1000.0
                    }
                }
            } else {
                max_position_value / price
            }
        } else {
            max_position_value / price
        }
    }

    /// Calculate stop loss price based on position and risk parameters
    pub fn calculate_stop_loss(
        &self,
        _position: &Position,
        entry_price: f64,
        is_long: bool,
    ) -> f64 {
        let stop_loss_percentage = self.config.stop_loss_percentage;

        if is_long {
            // Long position: stop loss below entry price
            entry_price * (1.0 - stop_loss_percentage)
        } else {
            // Short position: stop loss above entry price
            entry_price * (1.0 + stop_loss_percentage)
        }
    }

    /// Calculate take profit price based on position and risk parameters
    pub fn calculate_take_profit(
        &self,
        _position: &Position,
        entry_price: f64,
        is_long: bool,
    ) -> f64 {
        let take_profit_percentage = self.config.take_profit_percentage;

        if is_long {
            // Long position: take profit above entry price
            entry_price * (1.0 + take_profit_percentage)
        } else {
            // Short position: take profit below entry price
            entry_price * (1.0 - take_profit_percentage)
        }
    }

    /// Analyze portfolio risk and generate risk metrics
    pub fn analyze_portfolio_risk(&self, portfolio: &Portfolio) -> RiskMetrics {
        let portfolio_stats = portfolio.get_stats();
        let portfolio_value = portfolio_stats.total_value;

        // Calculate current exposure
        let mut current_exposure = 0.0;
        let mut positions_at_risk = 0;

        for position in portfolio.positions().values() {
            let position_value = position.quantity * position.current_price;
            current_exposure += position_value.abs();

            // Check if position needs risk management
            let position_percentage = position_value.abs() / portfolio_value;
            if position_percentage > (self.config.max_position_size / 100.0) {
                positions_at_risk += 1;
            }
        }

        let exposure_ratio = current_exposure / portfolio_value;
        let max_position_value = portfolio_value * (self.config.max_position_size / 100.0);
        let available_capital = portfolio_value - current_exposure;
        let risk_budget_used = exposure_ratio / self.config.max_portfolio_exposure;

        RiskMetrics {
            portfolio_value,
            max_position_value,
            current_exposure,
            available_capital,
            risk_budget_used,
            positions_at_risk,
        }
    }

    /// Analyze individual position risk
    pub fn analyze_position_risk(
        &self,
        portfolio: &Portfolio,
        symbol: &str,
    ) -> Option<PositionRisk> {
        let position = portfolio.get_position(symbol)?;
        let portfolio_stats = portfolio.get_stats();
        let portfolio_value = portfolio_stats.total_value;

        let current_value = position.quantity * position.current_price;
        let percentage_of_portfolio = current_value.abs() / portfolio_value;
        let is_long = position.quantity > 0.0;

        let stop_loss_price = self.calculate_stop_loss(position, position.average_cost, is_long);
        let take_profit_price =
            self.calculate_take_profit(position, position.average_cost, is_long);

        // Calculate risk amount (how much we could lose)
        let risk_amount = if is_long {
            position.quantity * (position.current_price - stop_loss_price)
        } else {
            position.quantity.abs() * (stop_loss_price - position.current_price)
        };

        let max_loss_percentage = risk_amount / portfolio_value;

        // Check if position needs risk management
        let needs_stop_loss = !self.stop_losses.contains_key(symbol);
        let needs_take_profit = !self.take_profits.contains_key(symbol);
        let exceeds_position_limit =
            percentage_of_portfolio > (self.config.max_position_size / 100.0);

        Some(PositionRisk {
            symbol: symbol.to_string(),
            current_value,
            percentage_of_portfolio,
            stop_loss_price,
            take_profit_price,
            risk_amount,
            max_loss_percentage,
            needs_stop_loss,
            needs_take_profit,
            exceeds_position_limit,
        })
    }

    /// Generate risk management signals
    pub fn generate_risk_signals(&self, portfolio: &Portfolio) -> Vec<RiskSignal> {
        let mut signals = Vec::new();
        let portfolio_stats = portfolio.get_stats();
        let portfolio_value = portfolio_stats.total_value;

        // Check overall portfolio exposure first
        let current_exposure = portfolio
            .positions()
            .values()
            .map(|p| (p.quantity * p.current_price).abs())
            .sum::<f64>();
        let exposure_ratio = current_exposure / portfolio_value;

        // If total exposure exceeds limit, generate reduction signals
        if exposure_ratio > self.config.max_portfolio_exposure {
            let excess_exposure =
                current_exposure - (portfolio_value * self.config.max_portfolio_exposure);

            // Sort positions by size (largest first) for systematic reduction
            let mut position_sizes: Vec<(&String, &Position, f64)> = portfolio
                .positions()
                .iter()
                .map(|(symbol, position)| {
                    let position_value = (position.quantity * position.current_price).abs();
                    (symbol, position, position_value)
                })
                .collect();
            position_sizes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

            let mut remaining_excess = excess_exposure;
            for (symbol, position, position_value) in position_sizes {
                if remaining_excess <= 0.0 {
                    break;
                }

                // Calculate how much to reduce this position
                let reduction_amount = remaining_excess.min(position_value * 0.5); // Reduce by up to 50%
                let reduce_quantity = reduction_amount / position.current_price;

                if reduce_quantity > 0.01 {
                    // Only if meaningful reduction
                    signals.push(RiskSignal {
                        symbol: symbol.to_string(),
                        action: RiskAction::ReducePosition,
                        quantity: reduce_quantity,
                        reason: format!(
                            "Portfolio exposure {:.1}% exceeds limit {:.1}% - reducing largest positions",
                            exposure_ratio * 100.0,
                            self.config.max_portfolio_exposure * 100.0
                        ),
                        urgency: RiskUrgency::Critical,
                    });

                    remaining_excess -= reduction_amount;
                }
            }
        }

        for (symbol, position) in portfolio.positions() {
            if let Some(position_risk) = self.analyze_position_risk(portfolio, symbol) {
                // Check for position size violations
                if position_risk.exceeds_position_limit {
                    let max_allowed_value =
                        portfolio_value * (self.config.max_position_size / 100.0);
                    let excess_value = position_risk.current_value - max_allowed_value;
                    let reduce_quantity = excess_value / position.current_price;

                    signals.push(RiskSignal {
                        symbol: symbol.to_string(),
                        action: RiskAction::ReducePosition,
                        quantity: reduce_quantity,
                        reason: format!(
                            "Position exceeds {}% limit: {:.2}% of portfolio",
                            self.config.max_position_size,
                            position_risk.percentage_of_portfolio * 100.0
                        ),
                        urgency: RiskUrgency::High,
                    });
                }

                // Check for missing stop losses
                if position_risk.needs_stop_loss {
                    signals.push(RiskSignal {
                        symbol: symbol.to_string(),
                        action: RiskAction::AddStopLoss,
                        quantity: position.quantity.abs(),
                        reason: format!(
                            "Missing stop loss at ${:.4}",
                            position_risk.stop_loss_price
                        ),
                        urgency: RiskUrgency::Medium,
                    });
                }

                // Check for missing take profits
                if position_risk.needs_take_profit {
                    signals.push(RiskSignal {
                        symbol: symbol.to_string(),
                        action: RiskAction::AddTakeProfit,
                        quantity: position.quantity.abs(),
                        reason: format!(
                            "Missing take profit at ${:.4}",
                            position_risk.take_profit_price
                        ),
                        urgency: RiskUrgency::Low,
                    });
                }

                // Check for excessive risk
                if position_risk.max_loss_percentage > 0.05 {
                    // 5% max loss per position
                    signals.push(RiskSignal {
                        symbol: symbol.to_string(),
                        action: RiskAction::ClosePosition,
                        quantity: position.quantity.abs(),
                        reason: format!(
                            "Excessive risk: {:.2}% potential loss",
                            position_risk.max_loss_percentage * 100.0
                        ),
                        urgency: RiskUrgency::Critical,
                    });
                }
            }
        }

        signals
    }

    /// Update stop loss levels
    pub fn update_stop_loss(&mut self, symbol: String, stop_loss_price: f64) {
        self.stop_losses.insert(symbol, stop_loss_price);
    }

    /// Update take profit levels
    pub fn update_take_profit(&mut self, symbol: String, take_profit_price: f64) {
        self.take_profits.insert(symbol, take_profit_price);
    }

    /// Check if a new position would violate risk limits
    pub fn validate_new_position(
        &self,
        portfolio: &Portfolio,
        _symbol: &str,
        quantity: f64,
        price: f64,
    ) -> Result<bool> {
        let portfolio_stats = portfolio.get_stats();
        let portfolio_value = portfolio_stats.total_value;
        let position_value = quantity * price;

        // Check position size limit
        let max_position_value = portfolio_value * (self.config.max_position_size / 100.0);
        if position_value > max_position_value {
            warn!(
                "Position size {} exceeds limit of ${:.2}",
                position_value, max_position_value
            );
            return Ok(false);
        }

        // Check portfolio exposure limit
        let current_exposure = portfolio
            .positions()
            .values()
            .map(|p| p.quantity * p.current_price)
            .sum::<f64>();
        let new_exposure = (current_exposure + position_value) / portfolio_value;

        if new_exposure > self.config.max_portfolio_exposure {
            warn!(
                "Portfolio exposure {:.2}% exceeds limit of {:.2}%",
                new_exposure * 100.0,
                self.config.max_portfolio_exposure * 100.0
            );
            return Ok(false);
        }

        // Check available cash
        if position_value > portfolio_stats.cash_balance {
            warn!(
                "Insufficient cash: need ${:.2}, have ${:.2}",
                position_value, portfolio_stats.cash_balance
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Log risk analysis
    pub fn log_risk_analysis(&self, portfolio: &Portfolio) {
        let risk_metrics = self.analyze_portfolio_risk(portfolio);

        info!("=== Risk Analysis ===");
        info!("Portfolio Value: ${:.2}", risk_metrics.portfolio_value);
        info!(
            "Current Exposure: ${:.2} ({:.1}%)",
            risk_metrics.current_exposure,
            (risk_metrics.current_exposure / risk_metrics.portfolio_value) * 100.0
        );
        info!(
            "Risk Budget Used: {:.1}%",
            risk_metrics.risk_budget_used * 100.0
        );
        info!("Positions at Risk: {}", risk_metrics.positions_at_risk);

        // Log individual position risks
        for symbol in portfolio.positions().keys() {
            if let Some(position_risk) = self.analyze_position_risk(portfolio, symbol) {
                info!(
                    "Position Risk - {}: {:.2}% of portfolio, SL: ${:.4}, TP: ${:.4}",
                    symbol,
                    position_risk.percentage_of_portfolio * 100.0,
                    position_risk.stop_loss_price,
                    position_risk.take_profit_price
                );
            }
        }

        // Log risk signals
        let signals = self.generate_risk_signals(portfolio);
        if !signals.is_empty() {
            warn!("Risk Signals Generated:");
            for signal in signals {
                match signal.urgency {
                    RiskUrgency::Critical => {
                        error!("CRITICAL: {} - {}", signal.symbol, signal.reason)
                    }
                    RiskUrgency::High => warn!("HIGH: {} - {}", signal.symbol, signal.reason),
                    RiskUrgency::Medium => warn!("MEDIUM: {} - {}", signal.symbol, signal.reason),
                    RiskUrgency::Low => info!("LOW: {} - {}", signal.symbol, signal.reason),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_position_size_calculation() {
        // Test basic position sizing logic
        assert!(true); // Placeholder
    }

    #[test]
    fn test_stop_loss_calculation() {
        // Test stop loss calculation
        assert!(true); // Placeholder
    }
}
