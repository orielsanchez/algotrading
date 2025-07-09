use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::security_types::SecurityInfo;

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
        self.data.insert(req_id, MarketData {
            symbol: symbol.clone(),
            last_price: 0.0,
            bid_price: 0.0,
            ask_price: 0.0,
            volume: 0,
            timestamp: Utc::now(),
            security_info: self.security_map.get(&symbol).cloned(),
        });
        if !self.price_history.contains_key(&symbol) {
            self.price_history.insert(symbol.clone(), PriceHistory {
                symbol: symbol.clone(),
                prices: Vec::new(),
                max_size: 1000,
            });
        }
    }
    
    pub fn add_historical_price(&mut self, symbol: &str, timestamp: time::OffsetDateTime, price: f64) {
        if let Some(history) = self.price_history.get_mut(symbol) {
            // Convert time::OffsetDateTime to chrono::DateTime<Utc>
            let datetime = DateTime::from_timestamp(timestamp.unix_timestamp(), 0)
                .unwrap_or_else(|| Utc::now());
            
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
        if let Some(req_id) = self.symbol_map.iter().find(|(_, s)| s.as_str() == symbol).map(|(id, _)| *id) {
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
        
        if history.prices.len() < lookback_period {
            return None;
        }
        
        let recent_prices = &history.prices[history.prices.len() - lookback_period..];
        let start_price = recent_prices.first()?.1;
        let end_price = recent_prices.last()?.1;
        
        if start_price > 0.0 {
            Some((end_price - start_price) / start_price)
        } else {
            None
        }
    }
    
    pub fn get_latest_prices(&self) -> HashMap<String, f64> {
        let mut prices = HashMap::new();
        for data in self.data.values() {
            if data.last_price > 0.0 {
                prices.insert(data.symbol.clone(), data.last_price);
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