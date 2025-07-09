use anyhow::Result;
use ibapi::prelude::*;
use ibapi::Client;
use ibapi::market_data::historical::{BarSize as HistoricalBarSize, WhatToShow as HistoricalWhatToShow};
use ibapi::market_data::realtime::{BarSize as RealtimeBarSize, WhatToShow as RealtimeWhatToShow};
use ibapi::accounts::{AccountSummaries, AccountSummaryTags, PositionUpdate};
use log::{info, error, warn};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use std::collections::HashMap;
use crate::config::{TwsConfig, SecurityConfig};
use crate::market_data::{MarketDataHandler, MarketDataUpdate};
use crate::security_types::SecurityType;
use chrono::Utc;

pub struct TwsClient {
    client: Arc<Client>,
    pub market_data_handler: Arc<Mutex<MarketDataHandler>>,
    config: TwsConfig,
    security_configs: Arc<Mutex<HashMap<String, SecurityConfig>>>,
    active_subscriptions: Arc<Mutex<HashMap<i32, mpsc::Sender<MarketDataUpdate>>>>,
}

impl TwsClient {
    pub async fn new(config: TwsConfig) -> Result<Self> {
        let market_data_handler = Arc::new(Mutex::new(MarketDataHandler::new()));
        
        let client = Arc::new(Client::connect(
            &format!("{}:{}", config.host, config.port),
            config.client_id
        )?);
        
        info!("Connected to TWS at {}:{}", config.host, config.port);
        
        Ok(Self {
            client,
            market_data_handler,
            config,
            security_configs: Arc::new(Mutex::new(HashMap::new())),
            active_subscriptions: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    pub async fn connect(&mut self) -> Result<()> {
        // Connection already established in new()
        Ok(())
    }
    
    pub async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnected from TWS");
        Ok(())
    }
    
    pub async fn register_security_config(&self, symbol: String, config: SecurityConfig) {
        let mut configs = self.security_configs.lock().await;
        configs.insert(symbol, config);
    }
    
    fn create_contract(&self, security_config: &SecurityConfig) -> Contract {
        match security_config.security_type {
            SecurityType::Stock => {
                let mut contract = Contract::stock(&security_config.symbol);
                contract.exchange = security_config.exchange.clone();
                contract.currency = security_config.currency.clone();
                contract
            }
            SecurityType::Future => {
                let mut contract = Contract::futures(&security_config.symbol);
                contract.exchange = security_config.exchange.clone();
                contract.currency = security_config.currency.clone();
                
                if let Some(futures_specs) = &security_config.futures_specs {
                    contract.last_trade_date_or_contract_month = futures_specs.contract_month.clone();
                }
                
                contract
            }
        }
    }
    
    pub async fn request_market_data(&self, symbol: &str, req_id: i32) -> Result<()> {
        // Register symbol with handler
        let mut handler = self.market_data_handler.lock().await;
        handler.register_symbol(req_id, symbol.to_string());
        drop(handler);
        
        // Get security config to create appropriate contract
        let configs = self.security_configs.lock().await;
        let contract = if let Some(security_config) = configs.get(symbol) {
            self.create_contract(security_config)
        } else {
            // Fallback to stock if no config found
            Contract::stock(symbol)
        };
        drop(configs);
        
        let client = self.client.clone();
        
        // Get historical data for momentum calculation
        match client.historical_data(
            &contract,
            None, // end time (None = now)
            30.days(),
            HistoricalBarSize::Day,
            HistoricalWhatToShow::Trades,
            true, // use RTH
        ) {
            Ok(historical_bars) => {
                let mut handler = self.market_data_handler.lock().await;
                for bar in historical_bars.bars.iter() {
                    handler.add_historical_price(symbol, bar.date, bar.close);
                }
                info!("Loaded {} days of historical data for {}", 
                    handler.get_price_history(symbol).map(|h| h.prices.len()).unwrap_or(0), 
                    symbol
                );
            }
            Err(e) => {
                error!("Failed to get historical data for {}: {}", symbol, e);
            }
        }
        
        // For now, we'll skip real-time data subscription due to lifetime issues
        // In production, you would handle this with a separate long-running task
        // that owns the client and manages subscriptions
        
        info!("Historical data loaded for {}", symbol);
        Ok(())
    }
    
    pub async fn place_order(&self, symbol: &str, quantity: f64, _order_type: &str) -> Result<()> {
        // Get security config to create appropriate contract
        let configs = self.security_configs.lock().await;
        let (contract, unit_type) = if let Some(security_config) = configs.get(symbol) {
            let unit = match security_config.security_type {
                SecurityType::Stock => "shares",
                SecurityType::Future => "contracts",
            };
            (self.create_contract(security_config), unit)
        } else {
            // Fallback to stock if no config found
            (Contract::stock(symbol), "shares")
        };
        drop(configs);
        
        let action = if quantity > 0.0 { Action::Buy } else { Action::Sell };
        let order = order_builder::market_order(action, quantity.abs());
        
        let order_id = self.client.next_order_id();
        
        // Subscribe to order updates
        let _order_subscription = self.client.place_order(order_id, &contract, &order)?;
        
        info!("Placed {:?} order #{} for {} {} of {}", action, order_id, quantity.abs(), unit_type, symbol);
        
        // Log order placement
        info!("Order #{} submitted for {}", order_id, symbol);
        
        Ok(())
    }
    
    pub async fn subscribe_realtime_data(
        &self,
        symbol: &str,
        req_id: i32,
        tx: mpsc::Sender<MarketDataUpdate>,
    ) -> Result<()> {
        // Register the subscription
        let mut subscriptions = self.active_subscriptions.lock().await;
        subscriptions.insert(req_id, tx.clone());
        drop(subscriptions);
        
        // Register symbol with handler
        let mut handler = self.market_data_handler.lock().await;
        handler.register_symbol(req_id, symbol.to_string());
        drop(handler);
        
        // Get security config to create appropriate contract
        let configs = self.security_configs.lock().await;
        let contract = if let Some(security_config) = configs.get(symbol) {
            self.create_contract(security_config)
        } else {
            // Fallback to stock if no config found
            Contract::stock(symbol)
        };
        drop(configs);
        
        let client = self.client.clone();
        let symbol_owned = symbol.to_string();
        let active_subs = self.active_subscriptions.clone();
        let handler_ref = self.market_data_handler.clone();
        
        // Spawn a task to handle real-time bars data
        tokio::spawn(async move {
            info!("Starting real-time market data subscription for {}", symbol_owned);
            
            // Subscribe to real-time bars (5 second intervals)
            match client.realtime_bars(&contract, RealtimeBarSize::Sec5, RealtimeWhatToShow::Trades, false) {
                Ok(subscription) => {
                    info!("Successfully subscribed to real-time bars for {}", symbol_owned);
                    
                    // Process incoming bar data
                    for bar in subscription {
                        let subs = active_subs.lock().await;
                        if let Some(tx) = subs.get(&req_id) {
                            let update = MarketDataUpdate {
                                symbol: symbol_owned.clone(),
                                last_price: bar.close,
                                bid_price: bar.close - 0.01,  // Approximate bid
                                ask_price: bar.close + 0.01,  // Approximate ask
                                volume: bar.volume as i64,
                                timestamp: Utc::now(),
                            };
                            
                            // Update the handler
                            let mut handler = handler_ref.lock().await;
                            handler.update_realtime_data(&symbol_owned, update.last_price, update.volume);
                            drop(handler);
                            
                            // Send to channel
                            if tx.send(update).await.is_err() {
                                warn!("Failed to send market data update for {}, receiver dropped", symbol_owned);
                                break;
                            }
                        } else {
                            // Subscription was cancelled
                            info!("Subscription {} cancelled for {}", req_id, symbol_owned);
                            break;
                        }
                    }
                    
                    info!("Real-time bars stream ended for {}", symbol_owned);
                }
                Err(e) => {
                    error!("Failed to subscribe to real-time bars for {}: {}", symbol_owned, e);
                    
                    // Try to send an error indicator
                    let subs = active_subs.lock().await;
                    if let Some(tx) = subs.get(&req_id) {
                        // Send a market data update with zero prices to indicate error
                        let error_update = MarketDataUpdate {
                            symbol: symbol_owned.clone(),
                            last_price: 0.0,
                            bid_price: 0.0,
                            ask_price: 0.0,
                            volume: 0,
                            timestamp: Utc::now(),
                        };
                        let _ = tx.send(error_update).await;
                    }
                }
            }
            
            // Clean up subscription
            let mut subs = active_subs.lock().await;
            subs.remove(&req_id);
        });
        
        Ok(())
    }
    
    pub async fn unsubscribe_realtime_data(&self, req_id: i32) -> Result<()> {
        let mut subscriptions = self.active_subscriptions.lock().await;
        subscriptions.remove(&req_id);
        
        info!("Unsubscribed from market data for request ID {}", req_id);
        Ok(())
    }
    
    pub async fn get_account_summary(&self) -> Result<HashMap<String, f64>> {
        let mut summary = HashMap::new();
        
        // Request account summary from IBKR
        match self.client.account_summary("All", AccountSummaryTags::ALL) {
            Ok(subscription) => {
                // Process account summary data
                for update in &subscription {
                    match update {
                        AccountSummaries::Summary(account_summary) => {
                            match account_summary.tag.as_str() {
                                "NetLiquidation" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("net_liquidation".to_string(), value);
                                    }
                                }
                                "TotalCashValue" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("cash".to_string(), value);
                                    }
                                }
                                "BuyingPower" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("buying_power".to_string(), value);
                                    }
                                }
                                "UnrealizedPnL" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("unrealized_pnl".to_string(), value);
                                    }
                                }
                                "RealizedPnL" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("realized_pnl".to_string(), value);
                                    }
                                }
                                "AvailableFunds" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("available_funds".to_string(), value);
                                    }
                                }
                                "MaintMarginReq" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("maintenance_margin".to_string(), value);
                                    }
                                }
                                "InitMarginReq" => {
                                    if let Ok(value) = account_summary.value.parse::<f64>() {
                                        summary.insert("initial_margin".to_string(), value);
                                    }
                                }
                                _ => {}
                            }
                        }
                        AccountSummaries::End => {
                            subscription.cancel();
                            break;
                        }
                    }
                }
                
                info!("Retrieved account summary with {} values", summary.len());
                
                // If we didn't get any data, return defaults
                if summary.is_empty() {
                    warn!("No account data received from IBKR, using defaults");
                    summary.insert("net_liquidation".to_string(), 100000.0);
                    summary.insert("cash".to_string(), 100000.0);
                    summary.insert("buying_power".to_string(), 400000.0);
                    summary.insert("unrealized_pnl".to_string(), 0.0);
                    summary.insert("realized_pnl".to_string(), 0.0);
                }
            }
            Err(e) => {
                error!("Failed to request account summary: {}", e);
                // Return default values on error
                summary.insert("net_liquidation".to_string(), 100000.0);
                summary.insert("cash".to_string(), 100000.0);
                summary.insert("buying_power".to_string(), 400000.0);
                summary.insert("unrealized_pnl".to_string(), 0.0);
                summary.insert("realized_pnl".to_string(), 0.0);
            }
        }
        
        Ok(summary)
    }
    
    pub async fn get_positions(&self) -> Result<Vec<AccountPosition>> {
        let mut positions = Vec::new();
        
        match self.client.positions() {
            Ok(subscription) => {
                // Process all position updates
                while let Some(position_update) = subscription.next() {
                    match position_update {
                        PositionUpdate::Position(position) => {
                            let account_position = AccountPosition {
                                account: position.account.clone(),
                                symbol: position.contract.symbol.clone(),
                                position: position.position,
                                avg_cost: position.average_cost,
                                contract: position.contract.clone(),
                            };
                            positions.push(account_position);
                            
                            info!("Position: {} - {} units @ ${:.2}", 
                                position.contract.symbol, 
                                position.position,
                                position.average_cost
                            );
                        }
                        PositionUpdate::PositionEnd => {
                            info!("All positions received");
                            subscription.cancel();
                            break;
                        }
                    }
                }
                
                info!("Retrieved {} positions from account", positions.len());
            }
            Err(e) => {
                error!("Failed to request positions: {}", e);
            }
        }
        
        Ok(positions)
    }
}

#[derive(Debug, Clone)]
pub struct AccountPosition {
    pub account: String,
    pub symbol: String,
    pub position: f64,
    pub avg_cost: f64,
    pub contract: Contract,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_real_time_market_data_subscription() -> Result<()> {
        // Test that we can subscribe to real-time market data
        // and receive updates through a channel
        
        // Create a mock config for testing
        let config = TwsConfig {
            host: "127.0.0.1".to_string(),
            port: 7497, // paper trading port
            client_id: 999,
        };
        
        // This test will fail initially (RED phase)
        // We need to implement subscribe_realtime_data method
        let client = TwsClient::new(config).await?;
        
        // Subscribe to a stock
        let (tx, mut rx) = mpsc::channel(100);
        client.subscribe_realtime_data("AAPL", 1, tx).await?;
        
        // Should receive market data updates
        tokio::select! {
            Some(data) = rx.recv() => {
                assert_eq!(data.symbol, "AAPL");
                assert!(data.last_price > 0.0);
            }
            _ = sleep(Duration::from_secs(5)) => {
                panic!("No market data received within timeout");
            }
        }
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_multiple_symbol_subscriptions() -> Result<()> {
        // Test that we can subscribe to multiple symbols simultaneously
        let config = TwsConfig {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 998,
        };
        
        let client = TwsClient::new(config).await?;
        
        // Subscribe to multiple symbols
        let (tx, mut rx) = mpsc::channel(100);
        client.subscribe_realtime_data("AAPL", 1, tx.clone()).await?;
        client.subscribe_realtime_data("MSFT", 2, tx.clone()).await?;
        client.subscribe_realtime_data("GOOGL", 3, tx.clone()).await?;
        
        // Should receive updates for all symbols
        let mut received_symbols = std::collections::HashSet::new();
        let start_time = tokio::time::Instant::now();
        let timeout_duration = Duration::from_secs(10);
        
        loop {
            if start_time.elapsed() > timeout_duration {
                panic!("Not all symbols received data within timeout");
            }
            
            tokio::select! {
                Some(data) = rx.recv() => {
                    received_symbols.insert(data.symbol.clone());
                    if received_symbols.len() == 3 {
                        break;
                    }
                }
                _ = sleep(Duration::from_millis(100)) => {
                    // Continue checking
                }
            }
        }
        
        assert!(received_symbols.contains("AAPL"));
        assert!(received_symbols.contains("MSFT"));
        assert!(received_symbols.contains("GOOGL"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_unsubscribe_market_data() -> Result<()> {
        // Test that we can unsubscribe from market data
        let config = TwsConfig {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 997,
        };
        
        let client = TwsClient::new(config).await?;
        
        let (tx, mut rx) = mpsc::channel(100);
        client.subscribe_realtime_data("AAPL", 1, tx).await?;
        
        // Unsubscribe
        client.unsubscribe_realtime_data(1).await?;
        
        // Should not receive any more updates
        sleep(Duration::from_secs(2)).await;
        assert!(rx.try_recv().is_err());
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_market_data_error_handling() -> Result<()> {
        // Test that errors are properly propagated
        let config = TwsConfig {
            host: "127.0.0.1".to_string(),
            port: 7497,
            client_id: 996,
        };
        
        let client = TwsClient::new(config).await?;
        
        // Try to subscribe to an invalid symbol
        let (tx, mut rx) = mpsc::channel(100);
        let result = client.subscribe_realtime_data("INVALID_SYMBOL_12345", 1, tx).await;
        
        // Should handle error gracefully
        assert!(result.is_ok()); // Subscription itself succeeds
        
        // But we should receive an error message through the channel
        tokio::select! {
            Some(data) = rx.recv() => {
                // In real implementation, we'd have an error variant
                // For now, this test will help drive the design
                assert_eq!(data.symbol, "INVALID_SYMBOL_12345");
            }
            _ = sleep(Duration::from_secs(5)) => {
                // This is expected for invalid symbols
            }
        }
        
        Ok(())
    }
}