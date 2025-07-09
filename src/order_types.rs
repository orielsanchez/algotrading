use anyhow::Result;
use ibapi::prelude::*;
use ibapi::orders::Order;
use serde::{Deserialize, Serialize};

/// Enhanced order types for systematic trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit { price: f64 },
    Stop { stop_price: f64 },
    StopLimit { stop_price: f64, limit_price: f64 },
    TrailingStop { trail_amount: f64 },
    TrailingStopLimit { trail_amount: f64, limit_price: f64 },
}

/// Time in force options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimeInForce {
    Day,
    GTC,  // Good Till Canceled
    IOC,  // Immediate or Cancel
    FOK,  // Fill or Kill
}

/// Enhanced order parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderParams {
    pub symbol: String,
    pub action: OrderAction,
    pub quantity: f64,
    pub order_type: OrderType,
    pub time_in_force: TimeInForce,
    pub outside_rth: bool,  // Outside Regular Trading Hours
    pub hidden: bool,       // Hidden order
    pub all_or_none: bool,  // All or None
}

/// Order action enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderAction {
    Buy,
    Sell,
}

impl From<OrderAction> for Action {
    fn from(action: OrderAction) -> Self {
        match action {
            OrderAction::Buy => Action::Buy,
            OrderAction::Sell => Action::Sell,
        }
    }
}

/// Enhanced order builder for systematic trading
pub struct EnhancedOrderBuilder;

impl EnhancedOrderBuilder {
    /// Create a market order
    pub fn market_order(action: OrderAction, quantity: f64) -> Order {
        let mut order = Order::default();
        order.action = action.into();
        order.total_quantity = quantity;
        order.order_type = "MKT".to_string();
        order.tif = "DAY".to_string();
        order
    }

    /// Create a limit order
    pub fn limit_order(action: OrderAction, quantity: f64, limit_price: f64) -> Order {
        let mut order = Order::default();
        order.action = action.into();
        order.total_quantity = quantity;
        order.order_type = "LMT".to_string();
        order.limit_price = Some(limit_price);
        order.tif = "DAY".to_string();
        order
    }

    /// Create a stop order (stop market)
    pub fn stop_order(action: OrderAction, quantity: f64, stop_price: f64) -> Order {
        let mut order = Order::default();
        order.action = action.into();
        order.total_quantity = quantity;
        order.order_type = "STP".to_string();
        order.aux_price = Some(stop_price);
        order.tif = "DAY".to_string();
        order
    }

    /// Create a stop-limit order
    pub fn stop_limit_order(
        action: OrderAction,
        quantity: f64,
        stop_price: f64,
        limit_price: f64,
    ) -> Order {
        let mut order = Order::default();
        order.action = action.into();
        order.total_quantity = quantity;
        order.order_type = "STP LMT".to_string();
        order.limit_price = Some(limit_price);
        order.aux_price = Some(stop_price);
        order.tif = "DAY".to_string();
        order
    }

    /// Create a trailing stop order
    pub fn trailing_stop_order(
        action: OrderAction,
        quantity: f64,
        trail_amount: f64,
        is_percentage: bool,
    ) -> Order {
        let mut order = Order::default();
        order.action = action.into();
        order.total_quantity = quantity;
        order.order_type = "TRAIL".to_string();
        
        if is_percentage {
            order.trail_stop_price = Some(trail_amount);
        } else {
            order.aux_price = Some(trail_amount);
        }
        
        order.tif = "DAY".to_string();
        order
    }

    /// Create a bracket order (parent + profit target + stop loss)
    pub fn bracket_order(
        action: OrderAction,
        quantity: f64,
        entry_price: f64,
        profit_target: f64,
        stop_loss: f64,
    ) -> Vec<Order> {
        let mut orders = Vec::new();

        // Parent order (limit order to enter position)
        let mut parent = Order::default();
        parent.action = action.clone().into();
        parent.total_quantity = quantity;
        parent.order_type = "LMT".to_string();
        parent.limit_price = Some(entry_price);
        parent.tif = "DAY".to_string();
        parent.transmit = false; // Don't transmit until children are attached
        orders.push(parent);

        // Profit target order (opposite action)
        let profit_action = match action {
            OrderAction::Buy => OrderAction::Sell,
            OrderAction::Sell => OrderAction::Buy,
        };
        let mut profit_order = Order::default();
        profit_order.action = profit_action.into();
        profit_order.total_quantity = quantity;
        profit_order.order_type = "LMT".to_string();
        profit_order.limit_price = Some(profit_target);
        profit_order.tif = "GTC".to_string();
        profit_order.parent_id = 0; // Will be set to parent order ID
        orders.push(profit_order);

        // Stop loss order (opposite action)
        let stop_action = match action {
            OrderAction::Buy => OrderAction::Sell,
            OrderAction::Sell => OrderAction::Buy,
        };
        let mut stop_order = Order::default();
        stop_order.action = stop_action.into();
        stop_order.total_quantity = quantity;
        stop_order.order_type = "STP".to_string();
        stop_order.aux_price = Some(stop_loss);
        stop_order.tif = "GTC".to_string();
        stop_order.parent_id = 0; // Will be set to parent order ID
        orders.push(stop_order);

        orders
    }

    /// Create order from parameters
    pub fn from_params(params: OrderParams) -> Result<Order> {
        let mut order = match params.order_type {
            OrderType::Market => Self::market_order(params.action, params.quantity),
            OrderType::Limit { price } => Self::limit_order(params.action, params.quantity, price),
            OrderType::Stop { stop_price } => Self::stop_order(params.action, params.quantity, stop_price),
            OrderType::StopLimit { stop_price, limit_price } => {
                Self::stop_limit_order(params.action, params.quantity, stop_price, limit_price)
            }
            OrderType::TrailingStop { trail_amount } => {
                Self::trailing_stop_order(params.action, params.quantity, trail_amount, false)
            }
            OrderType::TrailingStopLimit { trail_amount, limit_price } => {
                let mut order = Self::trailing_stop_order(params.action, params.quantity, trail_amount, false);
                order.limit_price = Some(limit_price);
                order.order_type = "TRAIL LIMIT".to_string();
                order
            }
        };

        // Apply time in force
        order.tif = match params.time_in_force {
            TimeInForce::Day => "DAY",
            TimeInForce::GTC => "GTC",
            TimeInForce::IOC => "IOC",
            TimeInForce::FOK => "FOK",
        }.to_string();

        // Apply additional parameters
        order.outside_rth = params.outside_rth;
        order.hidden = params.hidden;
        order.all_or_none = params.all_or_none;

        Ok(order)
    }
}

/// Helper functions for risk management orders
pub struct RiskOrders;

impl RiskOrders {
    /// Create a stop loss order for an existing position
    pub fn stop_loss_for_position(
        symbol: &str,
        position_size: f64,
        stop_price: f64,
        is_long: bool,
    ) -> OrderParams {
        let action = if is_long {
            OrderAction::Sell
        } else {
            OrderAction::Buy
        };

        OrderParams {
            symbol: symbol.to_string(),
            action,
            quantity: position_size.abs(),
            order_type: OrderType::Stop { stop_price },
            time_in_force: TimeInForce::GTC,
            outside_rth: false,
            hidden: false,
            all_or_none: false,
        }
    }

    /// Create a take profit order for an existing position
    pub fn take_profit_for_position(
        symbol: &str,
        position_size: f64,
        target_price: f64,
        is_long: bool,
    ) -> OrderParams {
        let action = if is_long {
            OrderAction::Sell
        } else {
            OrderAction::Buy
        };

        OrderParams {
            symbol: symbol.to_string(),
            action,
            quantity: position_size.abs(),
            order_type: OrderType::Limit { price: target_price },
            time_in_force: TimeInForce::GTC,
            outside_rth: false,
            hidden: false,
            all_or_none: false,
        }
    }

    /// Create a trailing stop order for an existing position
    pub fn trailing_stop_for_position(
        symbol: &str,
        position_size: f64,
        trail_amount: f64,
        is_long: bool,
    ) -> OrderParams {
        let action = if is_long {
            OrderAction::Sell
        } else {
            OrderAction::Buy
        };

        OrderParams {
            symbol: symbol.to_string(),
            action,
            quantity: position_size.abs(),
            order_type: OrderType::TrailingStop { trail_amount },
            time_in_force: TimeInForce::GTC,
            outside_rth: false,
            hidden: false,
            all_or_none: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_order_creation() {
        let order = EnhancedOrderBuilder::market_order(OrderAction::Buy, 100.0);
        assert_eq!(order.action, Action::Buy);
        assert_eq!(order.total_quantity, 100.0);
        assert_eq!(order.order_type, "MKT");
    }

    #[test]
    fn test_limit_order_creation() {
        let order = EnhancedOrderBuilder::limit_order(OrderAction::Buy, 100.0, 150.0);
        assert_eq!(order.action, Action::Buy);
        assert_eq!(order.total_quantity, 100.0);
        assert_eq!(order.order_type, "LMT");
        assert_eq!(order.limit_price, Some(150.0));
    }

    #[test]
    fn test_stop_order_creation() {
        let order = EnhancedOrderBuilder::stop_order(OrderAction::Sell, 100.0, 140.0);
        assert_eq!(order.action, Action::Sell);
        assert_eq!(order.total_quantity, 100.0);
        assert_eq!(order.order_type, "STP");
        assert_eq!(order.aux_price, Some(140.0));
    }

    #[test]
    fn test_bracket_order_creation() {
        let orders = EnhancedOrderBuilder::bracket_order(
            OrderAction::Buy,
            100.0,
            150.0,  // entry
            160.0,  // profit target
            140.0,  // stop loss
        );

        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].action, Action::Buy);      // Parent
        assert_eq!(orders[1].action, Action::Sell);     // Profit target
        assert_eq!(orders[2].action, Action::Sell);     // Stop loss
    }

    #[test]
    fn test_risk_orders() {
        let stop_loss = RiskOrders::stop_loss_for_position("AAPL", 100.0, 140.0, true);
        assert_eq!(stop_loss.action, OrderAction::Sell);
        assert_eq!(stop_loss.quantity, 100.0);
        
        let take_profit = RiskOrders::take_profit_for_position("AAPL", 100.0, 160.0, true);
        assert_eq!(take_profit.action, OrderAction::Sell);
        assert_eq!(take_profit.quantity, 100.0);
    }
}