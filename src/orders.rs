use anyhow::Result;
use chrono::{DateTime, Utc};
use log::info;
use crate::security_types::{SecurityInfo, SecurityType};

#[derive(Debug, Clone)]
pub struct OrderSignal {
    pub symbol: String,
    pub action: String,
    pub quantity: f64,
    pub price: f64,
    pub order_type: String,
    pub reason: String,
    pub security_info: SecurityInfo,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: i32,
    pub symbol: String,
    pub action: String,
    pub quantity: f64,
    pub order_type: String,
    pub limit_price: Option<f64>,
    pub stop_price: Option<f64>,
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
    pub security_info: SecurityInfo,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrderStatus {
    Pending,
    Submitted,
    Filled,
    PartiallyFilled,
    Cancelled,
    Rejected,
}

pub struct OrderManager {
    orders: Vec<Order>,
    next_order_id: i32,
}

impl OrderManager {
    pub fn new() -> Self {
        Self {
            orders: Vec::new(),
            next_order_id: 1000,
        }
    }
    
    pub fn create_order(&mut self, signal: OrderSignal) -> Order {
        let order = Order {
            id: self.next_order_id,
            symbol: signal.symbol.clone(),
            action: signal.action,
            quantity: signal.quantity,
            order_type: signal.order_type,
            limit_price: None,
            stop_price: None,
            status: OrderStatus::Pending,
            timestamp: Utc::now(),
            security_info: signal.security_info.clone(),
        };
        
        self.next_order_id += 1;
        self.orders.push(order.clone());
        
        let quantity_str = match signal.security_info.security_type {
            SecurityType::Stock => format!("{} shares", order.quantity),
            SecurityType::Future => format!("{} contracts", order.quantity),
        };
        
        info!("Created order #{}: {} {} of {} ({})", 
            order.id, order.action, quantity_str, order.symbol, signal.reason);
        
        order
    }
    
    pub fn update_order_status(&mut self, order_id: i32, status: OrderStatus) -> Result<()> {
        if let Some(order) = self.orders.iter_mut().find(|o| o.id == order_id) {
            order.status = status;
            info!("Order #{} status updated to {:?}", order_id, order.status);
            Ok(())
        } else {
            anyhow::bail!("Order {} not found", order_id)
        }
    }
    
    pub fn get_pending_orders(&self) -> Vec<&Order> {
        self.orders
            .iter()
            .filter(|o| o.status == OrderStatus::Pending || o.status == OrderStatus::Submitted)
            .collect()
    }
    
    pub fn get_order(&self, order_id: i32) -> Option<&Order> {
        self.orders.iter().find(|o| o.id == order_id)
    }
    
    pub fn cancel_order(&mut self, order_id: i32) -> Result<()> {
        self.update_order_status(order_id, OrderStatus::Cancelled)
    }
}