mod bollinger;
mod breakout;
mod config;
mod connection;
mod futures_utils;
mod margin;
mod market_data;
mod momentum;
mod order_types;
mod orders;
mod portfolio;
mod risk;
mod risk_budgeting;
mod security_types;
mod stats;
mod volatility;

use market_data::{MarketDataUpdate, TimeFrame};

use anyhow::Result;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Duration, interval, sleep};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with default info level if RUST_LOG not set
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info"); }
    }
    env_logger::init();
    info!("Starting Momentum Trading Bot");

    // Get config file from command line argument or use default
    let args: Vec<String> = env::args().collect();
    let config_file = if args.len() > 1 {
        &args[1]
    } else {
        "config.json"
    };

    info!("Loading configuration from: {}", config_file);
    let config = config::TradingConfig::load_from_file(config_file)?;

    // Create TWS client
    let tws_client = Arc::new(connection::TwsClient::new(config.tws_config.clone()).await?);

    // Initialize components
    let momentum_strategy = Arc::new(Mutex::new(momentum::MomentumStrategy::new(
        config.strategy_config.clone(),
    )));
    let order_manager = Arc::new(Mutex::new(orders::OrderManager::new()));
    let portfolio = Arc::new(Mutex::new(portfolio::Portfolio::new(100000.0)));
    let risk_manager = Arc::new(Mutex::new(risk::RiskManager::new(
        config.risk_config.clone(),
    )));
    
    // Initialize risk budgeting system
    let risk_budgeter = Arc::new(Mutex::new(risk_budgeting::RiskBudgeter::new(
        config.risk_config.clone(),
        config.risk_config.risk_budget_target_volatility,
    )));

    // Initialize market data handler with TwsClient
    let handler_guard = tws_client.market_data_handler.lock().await;

    // Register securities with market data handler and portfolio
    let mut port = portfolio.lock().await;
    for security_cfg in &config.strategy_config.securities {
        let security_info = match security_cfg.security_type {
            security_types::SecurityType::Future => {
                let contract = security_cfg.futures_specs.as_ref().map(|s| security_types::FuturesContract {
                    underlying: s.underlying.clone(),
                    expiry: s.expiry.clone(),
                    multiplier: s.multiplier,
                    tick_size: s.tick_size,
                    contract_month: s.contract_month.clone(),
                }).unwrap_or_default();
                
                security_types::SecurityInfo::new_future(
                    security_cfg.symbol.clone(),
                    security_cfg.exchange.clone(),
                    security_cfg.currency.clone(),
                    contract,
                )
            },
            security_types::SecurityType::Forex => security_types::SecurityInfo::new_forex(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            ),
            _ => security_types::SecurityInfo::new_stock(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            ),
        };

        port.register_security(security_cfg.symbol.clone(), security_info.clone());
    }
    drop(port);
    drop(handler_guard);

    // Request market data for all securities
    info!(
        "Subscribing to market data for {} securities",
        config.strategy_config.securities.len()
    );

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

    // Track subscribed symbols to avoid duplicates
    let mut subscribed_symbols = std::collections::HashSet::new();

    for (idx, security_cfg) in config.strategy_config.securities.iter().enumerate() {
        // Skip duplicate symbols to avoid multiple subscriptions
        if !subscribed_symbols.insert(security_cfg.symbol.clone()) {
            continue;
        }

        // Register security config with TwsClient
        tws_client
            .register_security_config(security_cfg.symbol.clone(), security_cfg.clone())
            .await;

        // Register with market data handler
        let mut handler_guard = tws_client.market_data_handler.lock().await;

        let security_info = match security_cfg.security_type {
            security_types::SecurityType::Future => {
                let contract = security_cfg.futures_specs.as_ref().map(|s| security_types::FuturesContract {
                    underlying: s.underlying.clone(),
                    expiry: s.expiry.clone(),
                    multiplier: s.multiplier,
                    tick_size: s.tick_size,
                    contract_month: s.contract_month.clone(),
                }).unwrap_or_default();
                
                security_types::SecurityInfo::new_future(
                    security_cfg.symbol.clone(),
                    security_cfg.exchange.clone(),
                    security_cfg.currency.clone(),
                    contract,
                )
            },
            security_types::SecurityType::Forex => security_types::SecurityInfo::new_forex(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            ),
            _ => security_types::SecurityInfo::new_stock(
                security_cfg.symbol.clone(),
                security_cfg.exchange.clone(),
                security_cfg.currency.clone(),
            ),
        };

        handler_guard.register_security(security_cfg.symbol.clone(), security_info);
        drop(handler_guard);

        // Subscribe to real-time data instead of request_market_data
        tws_client
            .subscribe_realtime_data(&security_cfg.symbol, idx as i32, tx.clone())
            .await?;
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
            let net_liq = summary.get("net_liquidation").copied().unwrap_or(0.0);
            let cash = summary.get("cash").copied().unwrap_or(0.0);
            let unrealized_pnl = summary.get("unrealized_pnl").copied().unwrap_or(0.0);
            
            info!("Account: Net=${:.2} Cash=${:.2} P&L=${:.2}", net_liq, cash, unrealized_pnl);
            
            // Update portfolio with actual cash balance
            let mut port = portfolio.lock().await;
            port.update_cash_balance(cash);
        }
        Err(e) => {
            error!("Failed to get account summary: {}", e);
        }
    }

    // Get current positions and sync with strategy and portfolio
    match tws_client.get_positions().await {
        Ok(positions) => {
            let position_count = positions.len();
            if !positions.is_empty() {
                let total_value: f64 = positions.iter().map(|p| p.position * p.avg_cost).sum();
                info!("Positions: {} open, total value ${:.2}", position_count, total_value);
                
                let mut strategy = momentum_strategy.lock().await;
                let mut port = portfolio.lock().await;

                // Get current market prices for position valuation
                let handler_guard = tws_client.market_data_handler.lock().await;
                let current_prices = handler_guard.get_latest_prices();
                drop(handler_guard);

                // Sync positions with both strategy and portfolio
                for pos in &positions {
                    // Sync with strategy
                    strategy.update_position(&pos.symbol, pos.position);

                    // Sync with portfolio using current market price
                    let current_price = current_prices
                        .get(&pos.symbol)
                        .copied()
                        .unwrap_or(pos.avg_cost);
                    port.sync_position_from_tws(pos, current_price);
                }
            } else {
                info!("No open positions");
                // Clear positions in portfolio if no TWS positions
                let mut port = portfolio.lock().await;
                port.sync_all_positions_from_tws(&[], &HashMap::new());
            }
        }
        Err(e) => {
            error!("Failed to get positions: {}", e);
        }
    }

    info!("Starting trading loop...");

    let mut trading_interval = interval(Duration::from_secs(
        config.strategy_config.rebalance_frequency_minutes * 60,
    ));
    let mut portfolio_update_interval = interval(Duration::from_secs(30)); // Update portfolio every 30 seconds

    loop {
        tokio::select! {
            _ = trading_interval.tick() => {
                info!("=== Running Enhanced Momentum Strategy ===");

                // Get market data handler from TwsClient
                let handler_guard = tws_client.market_data_handler.lock().await;

                // Show current market data status
                let latest_prices = handler_guard.get_latest_prices();
                debug!("Market data available for {} securities", latest_prices.len());
                for (symbol, price) in &latest_prices {
                    debug!("  {}: ${:.2}", symbol, price);
                }

                // Show detailed momentum analysis for each security (avoid duplicates)
                debug!("--- Enhanced Momentum Analysis ---");
                let mut processed_symbols = std::collections::HashSet::new();
                for security in &config.strategy_config.securities {
                    if !processed_symbols.insert(security.symbol.clone()) {
                        continue; // Skip duplicates
                    }

                    if let Some(multi_timeframe) = handler_guard.calculate_multi_timeframe_momentum(&security.symbol) {
                        debug!("{}:", security.symbol);
                        debug!("  Composite Score: {:.4}", multi_timeframe.composite_score);

                        if let Some(ref st) = multi_timeframe.timeframe_metrics.get(&TimeFrame::Days1) {
                            debug!("  Short-term (1d): momentum={:.4}, risk_adj={:.4}, vol={:.2}%, sharpe={:.2}",
                                st.simple_momentum, st.risk_adjusted_momentum, st.volatility * 100.0, st.sharpe_ratio);
                        }
                        if let Some(ref mt) = multi_timeframe.timeframe_metrics.get(&TimeFrame::Days7) {
                            debug!("  Medium-term (7d): momentum={:.4}, risk_adj={:.4}, vol={:.2}%, sharpe={:.2}",
                                mt.simple_momentum, mt.risk_adjusted_momentum, mt.volatility * 100.0, mt.sharpe_ratio);
                        }
                        if let Some(ref lt) = multi_timeframe.timeframe_metrics.get(&TimeFrame::Days14) {
                            debug!("  Long-term (14d): momentum={:.4}, risk_adj={:.4}, vol={:.2}%, sharpe={:.2}",
                                lt.simple_momentum, lt.risk_adjusted_momentum, lt.volatility * 100.0, lt.sharpe_ratio);
                        }
                    } else {
                        debug!("{}: Insufficient data for momentum calculation", security.symbol);
                    }
                }

                let mut strategy = momentum_strategy.lock().await;

                // Show current strategy positions for debugging
                let current_positions = strategy.get_positions();
                if !current_positions.is_empty() {
                    info!("Strategy tracked positions:");
                    for (symbol, quantity) in current_positions {
                        info!("  {} -> {:.0} units", symbol, quantity);
                    }
                } else {
                    info!("Strategy has no tracked positions");
                }

                let mut signals = strategy.calculate_signals(&handler_guard);

                drop(handler_guard);

                if !signals.is_empty() {
                    info!("Generated {} trading signals", signals.len());
                    for signal in &signals {
                        debug!("Signal: {} {} {:.0} shares @ ${:.2} - {}",
                            signal.action, signal.symbol, signal.quantity, signal.price, signal.reason);
                    }
                    
                    // Apply risk budgeting to signals if enabled
                    if config.risk_config.enable_risk_budgeting {
                        let port = portfolio.lock().await;
                        let budgeter = risk_budgeter.lock().await;
                        
                        // Calculate risk budget allocations
                        match budgeter.calculate_risk_contributions(&port) {
                            Ok(risk_contributions) => {
                                info!("Risk budgeting: {} positions analyzed", risk_contributions.risk_contributions.len());
                                
                                // Get ERC recommendations
                                match budgeter.calculate_erc_allocations(&port) {
                                    Ok(erc_allocations) => {
                                        info!("ERC allocation: {} target weights", erc_allocations.len());
                                        
                                        // Adjust signal quantities based on risk budgeting
                                        for signal in &mut signals {
                                            if let Some(erc_allocation) = erc_allocations.iter().find(|a| a.symbol == signal.symbol) {
                                                let portfolio_value = port.get_stats().total_value;
                                                let target_value = erc_allocation.target_weight * portfolio_value;
                                                let adjusted_quantity = target_value / signal.price;
                                                
                                                if adjusted_quantity.abs() < signal.quantity.abs() {
                                                    info!("Risk budgeting: Reducing {} position size from {:.0} to {:.0}",
                                                        signal.symbol, signal.quantity, adjusted_quantity);
                                                    signal.quantity = adjusted_quantity;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to calculate ERC allocation: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to calculate risk contributions: {}", e);
                            }
                        }
                        drop(port);
                    }

                    // Get current account summary for margin validation
                    let account_summary = match tws_client.get_account_summary().await {
                        Ok(summary) => summary,
                        Err(e) => {
                            error!("Failed to get account summary for margin validation: {}", e);
                            continue;
                        }
                    };

                    let mut order_mgr = order_manager.lock().await;
                    let mut port = portfolio.lock().await;
                    let risk_mgr = risk_manager.lock().await;

                    // Check if portfolio exposure is excessive before processing new signals
                    let current_exposure = port.positions()
                        .values()
                        .map(|p| (p.quantity * p.current_price).abs())
                        .sum::<f64>();
                    let portfolio_stats = port.get_stats();
                    let exposure_ratio = current_exposure / portfolio_stats.total_value;

                    if exposure_ratio > risk_mgr.config.max_portfolio_exposure {
                        warn!("Portfolio exposure {:.1}% exceeds {:.1}% limit - prioritizing risk reduction over new signals",
                            exposure_ratio * 100.0,
                            risk_mgr.config.max_portfolio_exposure * 100.0);

                        // Only process SELL signals (position reductions) when over-exposed
                        let reduction_signals: Vec<_> = signals.into_iter()
                            .filter(|s| s.action == "SELL")
                            .collect();

                        if !reduction_signals.is_empty() {
                            info!("Processing {} position reduction signals due to excessive exposure", reduction_signals.len());
                        } else {
                            warn!("No position reduction signals available - portfolio remains over-exposed");
                            drop(port);
                            drop(risk_mgr);
                            drop(order_mgr);
                            continue;
                        }

                        // Process only the reduction signals
                        for signal in reduction_signals {
                            // Execute reduction signals with minimal validation
                            info!("Executing risk reduction signal: {} {} {}",
                                signal.action, signal.quantity, signal.symbol);

                            match order_mgr.validate_and_create_order(
                                signal.clone(),
                                &port,
                                &account_summary,
                                config.risk_config.max_margin_utilization,
                            ) {
                                Ok(order) => {
                                    info!("Created risk reduction order #{}: {} {} {}",
                                        order.id, order.action, order.quantity, order.symbol);
                                    match tws_client.place_order_from_order(&order).await {
                                        Ok(tws_order_id) => {
                                            let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Submitted);
                                            info!("Risk reduction order submitted to TWS: {} {} {} (TWS ID: {})", order.action, order.quantity, order.symbol, tws_order_id);
                                        }
                                        Err(e) => {
                                            error!("Failed to place risk reduction order: {}", e);
                                            let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Rejected);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to create risk reduction order: {}", e);
                                }
                            }
                        }

                        drop(port);
                        drop(risk_mgr);
                        drop(order_mgr);
                        continue; // Skip normal signal processing
                    }

                    // Normal signal processing when exposure is within limits
                    // Perform risk analysis before executing signals
                    risk_mgr.log_risk_analysis(&port);

                    for signal in signals {
                        // Validate position against risk limits
                        let risk_validation = risk_mgr.validate_new_position(
                            &port,
                            &signal.symbol,
                            signal.quantity,
                            signal.price
                        );

                        if let Err(e) = risk_validation {
                            error!("Risk validation failed for {}: {}", signal.symbol, e);
                            continue;
                        }

                        // Additional risk budgeting validation if enabled
                        if config.risk_config.enable_risk_budgeting {
                            let budgeter = risk_budgeter.lock().await;
                            
                            // Check correlation risk
                            let symbols: Vec<String> = port.positions().keys().cloned().collect();
                            match budgeter.calculate_correlation_risk(&symbols) {
                                Ok(correlation_risk) => {
                                    if correlation_risk.diversification_score < (1.0 - config.risk_config.max_correlation_exposure) {
                                        warn!("Risk budgeting: Diversification score too low for {}: {:.2}% < {:.2}%",
                                            signal.symbol, 
                                            correlation_risk.diversification_score * 100.0,
                                            (1.0 - config.risk_config.max_correlation_exposure) * 100.0);
                                        drop(budgeter);
                                        continue;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to calculate correlation risk for {}: {}", signal.symbol, e);
                                }
                            }
                            drop(budgeter);
                        }

                        if !risk_validation.unwrap_or(false) {
                            info!("Order rejected by risk manager: {} {} {}",
                                signal.action, signal.quantity, signal.symbol);
                            continue;
                        }

                        // Validate margin and create order
                        match order_mgr.validate_and_create_order(
                            signal.clone(),
                            &port,
                            &account_summary,
                            config.risk_config.max_margin_utilization,
                        ) {
                            Ok(order) => {
                                match tws_client.place_order(&signal).await {
                                    Ok(tws_order_id) => {
                                        let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Submitted);
                                        // NOTE: Don't update portfolio here - wait for TWS position sync
                                        // Portfolio will be updated when TWS confirms the position change
                                        info!("Order submitted to TWS: {} {} {} (TWS ID: {})", signal.action, signal.quantity, signal.symbol, tws_order_id);
                                    }
                                    Err(e) => {
                                        error!("Failed to place order: {}", e);
                                        let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Rejected);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to create order due to margin constraints: {}", e);
                            }
                        }
                    }

                    // Update portfolio with latest prices after all trading is done
                    port.update_market_prices(&latest_prices);

                    // IMPORTANT: Force portfolio sync with actual TWS positions
                    if let Ok(tws_positions) = tws_client.get_positions().await {
                        port.sync_all_positions_from_tws(&tws_positions, &latest_prices);

                        // Also sync strategy positions with actual TWS positions
                        // Clear current positions and resync with TWS
                        for symbol in strategy.get_positions().keys().cloned().collect::<Vec<_>>() {
                            strategy.update_position(&symbol, 0.0); // Clear position
                        }
                        for pos in &tws_positions {
                            strategy.update_position(&pos.symbol, pos.position);
                        }
                    }

                    let stats = port.get_stats();
                    info!("Portfolio: ${:.2} total, {} positions, P&L ${:.2}", 
                        stats.total_value, stats.positions_count, stats.total_unrealized_pnl);

                    // Show current positions from portfolio (should match TWS now)
                    let positions = port.get_all_positions();
                    if !positions.is_empty() {
                        for (symbol, position) in positions {
                            let position_value = position.quantity * position.current_price;
                            debug!("  {}: {:.0} shares @ ${:.2} = ${:.2}",
                                symbol, position.quantity, position.current_price, position_value);
                        }
                    }
                } else {
                    info!("No trading signals generated");
                    debug!("Reasons: Securities may not meet momentum threshold or quality filters");

                    // Show current positions even when no signals
                    let positions = strategy.get_positions();
                    if !positions.is_empty() {
                        debug!("Maintaining {} existing positions", positions.len());
                        for (symbol, quantity) in positions {
                            if let Some(price) = latest_prices.get(symbol) {
                                let position_value = quantity * price;
                                info!("  {}: {:.0} shares @ ${:.2} = ${:.2}",
                                    symbol, quantity, price, position_value);
                            }
                        }
                    } else {
                        info!("No positions currently held");
                    }
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
                    info!("Portfolio: ${:.2} total, P&L ${:.2}",
                        stats.total_value, stats.total_unrealized_pnl);
                }

                // Also fetch updated account data and positions
                if let Ok(summary) = tws_client.get_account_summary().await {
                    if let (Some(net_liq), Some(unrealized_pnl)) =
                        (summary.get("net_liquidation"), summary.get("unrealized_pnl")) {
                        info!("Account: ${:.2} net, P&L ${:.2}",
                            net_liq, unrealized_pnl);
                    }

                    // Update cash balance
                    if let Some(cash) = summary.get("cash") {
                        let mut port = portfolio.lock().await;
                        port.update_cash_balance(*cash);
                    }

                    // Periodically sync positions from TWS
                    if let Ok(positions) = tws_client.get_positions().await {
                        let mut port = portfolio.lock().await;
                        let mut strategy = momentum_strategy.lock().await;

                        for pos in &positions {
                            // Sync with strategy
                            strategy.update_position(&pos.symbol, pos.position);

                            // Sync with portfolio using current market price
                            let current_price = latest_prices.get(&pos.symbol)
                                .copied()
                                .unwrap_or(pos.avg_cost);
                            port.sync_position_from_tws(pos, current_price);
                        }
                        
                        // Show current positions summary
                        if !positions.is_empty() {
                            let total_value: f64 = positions.iter().map(|p| p.position * p.avg_cost).sum();
                            info!("Positions: {} open, ${:.2} total", positions.len(), total_value);
                        } else {
                            info!("Positions: None");
                        }
                    }

                    // Check margin health and update portfolio margin statistics
                    let mut port = portfolio.lock().await;
                    let risk_mgr = risk_manager.lock().await;
                    let margin_status = margin::check_margin_health(
                        &port,
                        &summary,
                        config.risk_config.margin_call_threshold,
                    );

                    match margin_status {
                        margin::MarginStatus::Healthy => {
                            // No action needed
                        }
                        margin::MarginStatus::Warning(msg) => {
                            log::warn!("Margin warning: {}", msg);
                        }
                        margin::MarginStatus::Critical(msg) => {
                            log::error!("CRITICAL MARGIN ALERT: {}", msg);
                            // TODO: Implement risk reduction or trading halt
                        }
                    }

                    // Update portfolio margin statistics
                    port.update_margin_stats(&summary);

                    // Calculate and update margin for each position
                    let positions = port.positions().clone();
                    for (symbol, position) in positions.iter() {
                        if let Some(ref security_info) = position.security_info {
                            if let Ok(initial_margin) = margin::calculate_initial_margin(
                                security_info,
                                position.quantity,
                                position.current_price,
                            ) {
                                if let Ok(maintenance_margin) = margin::calculate_maintenance_margin(
                                    security_info,
                                    position.quantity,
                                    position.current_price,
                                ) {
                                    port.update_position_margin(symbol, initial_margin, maintenance_margin);
                                }
                            }
                        }
                    }
                    port.recalculate_margin_totals();

                    // Generate and execute risk signals
                    let risk_signals = risk_mgr.generate_risk_signals(&port);
                    if !risk_signals.is_empty() {
                        debug!("Generated {} risk management signals", risk_signals.len());

                        // Separate critical/high risk signals that need immediate execution
                        let mut critical_signals = Vec::new();

                        for risk_signal in risk_signals {
                            match risk_signal.urgency {
                                risk::RiskUrgency::Critical => {
                                    error!("CRITICAL RISK: {} - {}", risk_signal.symbol, risk_signal.reason);
                                    if matches!(risk_signal.action, risk::RiskAction::ReducePosition) {
                                        critical_signals.push(risk_signal);
                                    }
                                }
                                risk::RiskUrgency::High => {
                                    warn!("HIGH RISK: {} - {}", risk_signal.symbol, risk_signal.reason);
                                    if matches!(risk_signal.action, risk::RiskAction::ReducePosition) {
                                        critical_signals.push(risk_signal);
                                    }
                                }
                                risk::RiskUrgency::Medium => {
                                    warn!("MEDIUM RISK: {} - {}", risk_signal.symbol, risk_signal.reason);
                                }
                                risk::RiskUrgency::Low => {
                                    info!("LOW RISK: {} - {}", risk_signal.symbol, risk_signal.reason);
                                }
                            }
                        }

                        // Execute critical risk reduction orders immediately
                        if !critical_signals.is_empty() {
                            warn!("Executing {} critical risk reduction orders", critical_signals.len());
                            let mut order_mgr = order_manager.lock().await;

                            for risk_signal in critical_signals {
                                // Create risk reduction order
                                let reduction_signal = orders::OrderSignal {
                                    symbol: risk_signal.symbol.clone(),
                                    action: "SELL".to_string(), // Always sell to reduce exposure
                                    quantity: risk_signal.quantity,
                                    price: 0.0, // Market order - price will be filled by market
                                    order_type: "MKT".to_string(),
                                    limit_price: None, // Market order - no limit price
                                    reason: format!("RISK REDUCTION: {}", risk_signal.reason),
                                    security_info: portfolio.lock().await
                                        .get_position(&risk_signal.symbol)
                                        .and_then(|p| p.security_info.clone())
                                        .unwrap_or_else(|| {
                                            // Fallback to basic security info
                                            security_types::SecurityInfo::new_forex(
                                                risk_signal.symbol.clone(),
                                                "IDEALPRO".to_string(),
                                                "USD".to_string()
                                            )
                                        }),
                                };

                                info!("RISK REDUCTION ORDER: {} {} {} - {}",
                                    reduction_signal.action,
                                    reduction_signal.quantity,
                                    reduction_signal.symbol,
                                    reduction_signal.reason
                                );

                                // Create and execute the risk reduction order
                                let order = order_mgr.create_order(reduction_signal.clone());
                                info!("Created risk reduction order #{} for {}", order.id, order.symbol);

                                // Execute the order through TWS
                                match tws_client.place_order_from_order(&order).await {
                                    Ok(tws_order_id) => {
                                        info!("Successfully submitted risk reduction order for {} (TWS ID: {})", order.symbol, tws_order_id);
                                        let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Submitted);
                                        // NOTE: Don't update portfolio here - wait for TWS position sync
                                        // Portfolio will be updated when TWS confirms the position change
                                    }
                                    Err(e) => {
                                        error!("Failed to execute risk reduction order: {}", e);
                                        let _ = order_mgr.update_order_status(order.id, orders::OrderStatus::Rejected);
                                    }
                                }
                            }
                        }
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
    // Try to get exclusive access to disconnect, but don't panic if other refs exist
    if let Ok(mut tws_client_mut) = Arc::try_unwrap(tws_client) {
        tws_client_mut.disconnect().await?;
    } else {
        warn!("Could not get exclusive access to TwsClient for disconnect");
    }

    info!("Bot disconnected");
    Ok(())
}

// TwsClient is not cloneable by design to prevent multiple concurrent access
