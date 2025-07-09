mod config;
mod connection;
mod market_data;
mod momentum;
mod orders;
mod portfolio;
mod security_types;
mod futures_utils;

use market_data::MarketDataUpdate;

use anyhow::Result;
use log::{info, error};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{interval, Duration, sleep};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting Momentum Trading Bot");

    let config = config::TradingConfig::load()?;
    
    // Create TWS client
    let tws_client = Arc::new(connection::TwsClient::new(config.tws_config.clone()).await?);
    
    // Initialize components
    let momentum_strategy = Arc::new(Mutex::new(momentum::MomentumStrategy::new(config.strategy_config.clone())));
    let order_manager = Arc::new(Mutex::new(orders::OrderManager::new()));
    let portfolio = Arc::new(Mutex::new(portfolio::Portfolio::new(100000.0)));
    
    // Initialize market data handler with TwsClient
    let handler_guard = tws_client.market_data_handler.lock().await;
    
    // Register securities with market data handler and portfolio
    let mut port = portfolio.lock().await;
    for security_cfg in &config.strategy_config.securities {
        let security_info = if security_cfg.security_type == security_types::SecurityType::Future {
            security_types::SecurityInfo::new_future(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
                security_cfg.futures_specs.as_ref().map(|s| s.underlying.clone()).unwrap_or_default(),
                security_cfg.futures_specs.as_ref().map(|s| s.expiry.clone()).unwrap_or_default(),
                security_cfg.futures_specs.as_ref().map(|s| s.multiplier).unwrap_or(1.0),
                security_cfg.futures_specs.as_ref().map(|s| s.tick_size).unwrap_or(0.01),
                security_cfg.futures_specs.as_ref().map(|s| s.contract_month.clone()).unwrap_or_default(),
            )
        } else {
            security_types::SecurityInfo::new_stock(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            )
        };
        
        port.register_security(security_cfg.symbol.clone(), security_info.clone());
    }
    drop(port);
    drop(handler_guard);
    
    // Request market data for all securities
    info!("Subscribing to market data for {} securities", config.strategy_config.securities.len());
    
    // Create a channel to receive market data updates
    let (tx, mut rx) = mpsc::channel::<MarketDataUpdate>(1000);
    
    // Spawn a task to process market data updates
    let market_data_handler = tws_client.market_data_handler.clone();
    tokio::spawn(async move {
        while let Some(update) = rx.recv().await {
            let mut handler = market_data_handler.lock().await;
            handler.update_realtime_data(&update.symbol, update.last_price, update.volume);
        }
    });
    
    for (idx, security_cfg) in config.strategy_config.securities.iter().enumerate() {
        // Register security config with TwsClient
        tws_client.register_security_config(security_cfg.symbol.clone(), security_cfg.clone()).await;
        
        // Register with market data handler
        let mut handler_guard = tws_client.market_data_handler.lock().await;
        
        let security_info = if security_cfg.security_type == security_types::SecurityType::Future {
            security_types::SecurityInfo::new_future(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
                security_cfg.futures_specs.as_ref().map(|s| s.underlying.clone()).unwrap_or_default(),
                security_cfg.futures_specs.as_ref().map(|s| s.expiry.clone()).unwrap_or_default(),
                security_cfg.futures_specs.as_ref().map(|s| s.multiplier).unwrap_or(1.0),
                security_cfg.futures_specs.as_ref().map(|s| s.tick_size).unwrap_or(0.01),
                security_cfg.futures_specs.as_ref().map(|s| s.contract_month.clone()).unwrap_or_default(),
            )
        } else {
            security_types::SecurityInfo::new_stock(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            )
        };
        
        handler_guard.register_security(security_cfg.symbol.clone(), security_info);
        drop(handler_guard);
        
        // Subscribe to real-time data instead of request_market_data
        tws_client.subscribe_realtime_data(&security_cfg.symbol, idx as i32, tx.clone()).await?;
        info!("Subscribed to real-time data for {}", security_cfg.symbol);
        
        // Small delay between subscriptions to avoid overwhelming the API
        sleep(Duration::from_millis(100)).await;
    }
    
    // Wait for market data to populate
    info!("Waiting for market data to populate...");
    sleep(Duration::from_secs(5)).await;
    
    // Get initial account summary
    match tws_client.get_account_summary().await {
        Ok(summary) => {
            info!("Account Summary:");
            if let Some(net_liq) = summary.get("net_liquidation") {
                info!("  Net Liquidation: ${:.2}", net_liq);
            }
            if let Some(cash) = summary.get("cash") {
                info!("  Cash Balance: ${:.2}", cash);
                // Update portfolio with actual cash balance
                let mut port = portfolio.lock().await;
                *port = portfolio::Portfolio::new(*cash);
            }
            if let Some(buying_power) = summary.get("buying_power") {
                info!("  Buying Power: ${:.2}", buying_power);
            }
            if let Some(unrealized_pnl) = summary.get("unrealized_pnl") {
                info!("  Unrealized P&L: ${:.2}", unrealized_pnl);
            }
            if let Some(realized_pnl) = summary.get("realized_pnl") {
                info!("  Realized P&L: ${:.2}", realized_pnl);
            }
            if let Some(available_funds) = summary.get("available_funds") {
                info!("  Available Funds: ${:.2}", available_funds);
            }
            if let Some(maint_margin) = summary.get("maintenance_margin") {
                info!("  Maintenance Margin: ${:.2}", maint_margin);
            }
            if let Some(init_margin) = summary.get("initial_margin") {
                info!("  Initial Margin: ${:.2}", init_margin);
            }
        }
        Err(e) => {
            error!("Failed to get account summary: {}", e);
        }
    }
    
    // Get current positions
    match tws_client.get_positions().await {
        Ok(positions) => {
            if !positions.is_empty() {
                info!("Current Positions:");
                for pos in &positions {
                    let position_value = pos.position * pos.avg_cost;
                    info!("  {} - {} units @ ${:.2} = ${:.2}", 
                        pos.symbol, pos.position, pos.avg_cost, position_value
                    );
                }
            } else {
                info!("No open positions");
            }
        }
        Err(e) => {
            error!("Failed to get positions: {}", e);
        }
    }
    
    info!("Starting trading loop...");
    
    let mut trading_interval = interval(Duration::from_secs(config.strategy_config.rebalance_frequency_minutes * 60));
    let mut portfolio_update_interval = interval(Duration::from_secs(30)); // Update portfolio every 30 seconds
    
    loop {
        tokio::select! {
            _ = trading_interval.tick() => {
                info!("Running momentum strategy...");
                
                // Get market data handler from TwsClient
                let handler_guard = tws_client.market_data_handler.lock().await;
                let mut strategy = momentum_strategy.lock().await;
                let signals = strategy.calculate_signals(&handler_guard);
                
                // Get latest prices for portfolio update
                let latest_prices = handler_guard.get_latest_prices();
                drop(handler_guard);
                
                if !signals.is_empty() {
                    info!("Generated {} trading signals", signals.len());
                    
                    let mut order_mgr = order_manager.lock().await;
                    let mut port = portfolio.lock().await;
                    
                    for signal in signals {
                        let order = order_mgr.create_order(signal.clone());
                        
                        match tws_client.place_order(&signal.symbol, signal.quantity, &signal.order_type).await {
                            Ok(_) => {
                                order_mgr.update_order_status(order.id, orders::OrderStatus::Submitted)?;
                                port.update_position(signal.symbol.clone(), signal.quantity, signal.price);
                                strategy.update_position(signal.symbol, signal.quantity);
                            }
                            Err(e) => {
                                error!("Failed to place order: {}", e);
                                order_mgr.update_order_status(order.id, orders::OrderStatus::Rejected)?;
                            }
                        }
                    }
                    
                    // Update portfolio with latest prices
                    port.update_market_prices(&latest_prices);
                    let stats = port.get_stats();
                    info!("Portfolio stats - Total Value: ${:.2}, Cash: ${:.2}, Positions: {}, Unrealized P&L: ${:.2}", 
                        stats.total_value, stats.cash_balance, stats.positions_count, stats.total_unrealized_pnl);
                } else {
                    info!("No trading signals generated");
                }
            }
            _ = portfolio_update_interval.tick() => {
                // Periodically update portfolio with latest prices
                let handler_guard = tws_client.market_data_handler.lock().await;
                let latest_prices = handler_guard.get_latest_prices();
                drop(handler_guard);
                
                if !latest_prices.is_empty() {
                    let mut port = portfolio.lock().await;
                    port.update_market_prices(&latest_prices);
                    let stats = port.get_stats();
                    log::debug!("Portfolio update - Total Value: ${:.2}, Unrealized P&L: ${:.2}", 
                        stats.total_value, stats.total_unrealized_pnl);
                }
                
                // Also fetch updated account data
                if let Ok(summary) = tws_client.get_account_summary().await {
                    if let (Some(net_liq), Some(unrealized_pnl)) = 
                        (summary.get("net_liquidation"), summary.get("unrealized_pnl")) {
                        log::debug!("Account update - Net Liq: ${:.2}, Unrealized P&L: ${:.2}", 
                            net_liq, unrealized_pnl);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down momentum trading bot...");
                break;
            }
        }
    }
    
    // Cleanup
    let mut tws_client_mut = Arc::try_unwrap(tws_client)
        .unwrap_or_else(|arc| (*arc).clone());
    tws_client_mut.disconnect().await?;
    
    info!("Bot disconnected");
    Ok(())
}

// Need to implement Clone for TwsClient to handle the Arc unwrap
impl Clone for connection::TwsClient {
    fn clone(&self) -> Self {
        panic!("TwsClient cannot be cloned")
    }
}