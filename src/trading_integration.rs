use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::RiskConfig;
use crate::orders::OrderSignal;
use crate::portfolio::Portfolio;
use crate::position_inertia::{InertiaConfig, InertiaDecision, PositionInertiaCalculator};
use crate::security_types::SecurityType;
use crate::transaction_cost::{TransactionCostCalculator, TransactionCostConfig};

pub struct TradingIntegrationLayer {
    transaction_cost_calculator: Arc<Mutex<TransactionCostCalculator>>,
    position_inertia_calculator: Arc<Mutex<PositionInertiaCalculator>>,
    enable_transaction_cost_optimization: bool,
    enable_position_inertia: bool,
}

#[derive(Debug, Clone)]
pub struct SignalFilterResult {
    pub original_signals: usize,
    pub inertia_filtered: usize,
    pub cost_filtered: usize,
    pub final_signals: usize,
    pub total_estimated_costs: f64,
}

impl TradingIntegrationLayer {
    pub fn new(risk_config: &RiskConfig) -> Self {
        // Create transaction cost configuration
        let mut bid_ask_spreads = HashMap::new();
        bid_ask_spreads.insert("DEFAULT".to_string(), 0.0010); // Default 0.10% spread

        let mut commission_rates = HashMap::new();
        commission_rates.insert(SecurityType::Stock, risk_config.stock_commission);
        commission_rates.insert(SecurityType::Future, risk_config.futures_commission);
        commission_rates.insert(SecurityType::Forex, risk_config.forex_commission);

        let transaction_cost_config = TransactionCostConfig {
            bid_ask_spreads,
            commission_rates,
            market_impact_threshold: 0.01, // 1% of daily volume
            market_impact_coefficient: 0.5,
        };

        let transaction_cost_calculator = Arc::new(Mutex::new(TransactionCostCalculator::new(
            transaction_cost_config,
        )));

        // Create position inertia configuration
        let inertia_config = InertiaConfig {
            inertia_multiplier: risk_config.inertia_multiplier,
            min_position_change_value: risk_config.min_position_change_value,
            max_position_change_pct: risk_config.max_position_change_pct,
            enable_position_inertia: risk_config.enable_position_inertia,
        };

        let position_inertia_calculator =
            Arc::new(Mutex::new(PositionInertiaCalculator::new(inertia_config)));

        Self {
            transaction_cost_calculator,
            position_inertia_calculator,
            enable_transaction_cost_optimization: risk_config.enable_transaction_cost_optimization,
            enable_position_inertia: risk_config.enable_position_inertia,
        }
    }

    /// Filter signals through position inertia and transaction cost optimization
    pub async fn filter_signals_with_cost_optimization(
        &self,
        mut signals: Vec<OrderSignal>,
        portfolio: &Portfolio,
        latest_prices: &HashMap<String, f64>,
        max_acceptable_cost_bps: f64,
    ) -> Result<(Vec<OrderSignal>, SignalFilterResult)> {
        let original_count = signals.len();
        let mut inertia_filtered_count = 0;
        let mut cost_filtered_count = 0;
        let mut total_estimated_costs = 0.0;

        if !self.enable_position_inertia && !self.enable_transaction_cost_optimization {
            debug!(
                "Position inertia and transaction cost optimization disabled, returning original signals"
            );
            return Ok((
                signals,
                SignalFilterResult {
                    original_signals: original_count,
                    inertia_filtered: 0,
                    cost_filtered: 0,
                    final_signals: original_count,
                    total_estimated_costs: 0.0,
                },
            ));
        }

        // Step 1: Apply position inertia filtering
        if self.enable_position_inertia {
            let inertia_calculator = self.position_inertia_calculator.lock().await;
            let mut filtered_signals = Vec::new();

            for signal in signals {
                let current_position_shares = portfolio
                    .get_position(&signal.symbol)
                    .map(|p| p.quantity)
                    .unwrap_or(0.0);

                let target_position_shares = if signal.action == "BUY" {
                    signal.quantity
                } else {
                    -signal.quantity
                };

                // Convert to position values for inertia calculation
                let current_price = *latest_prices.get(&signal.symbol).unwrap_or(&signal.price);
                let current_position = current_position_shares * current_price;
                let target_position = target_position_shares * signal.price;

                // Estimate transaction cost for inertia calculation
                let estimated_cost = self
                    .estimate_transaction_cost(
                        &signal,
                        latest_prices.get(&signal.symbol).unwrap_or(&signal.price),
                    )
                    .await?;

                // Extract signal strength from reason field or use default
                let signal_strength = signal
                    .reason
                    .split("strength:")
                    .nth(1)
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .unwrap_or(10.0); // Default moderate signal strength


                let decision = inertia_calculator.calculate_position_decision(
                    current_position,
                    target_position,
                    estimated_cost,
                    signal_strength,
                    signal.price,
                )?;

                match decision.action {
                    InertiaDecision::Rebalance => {
                        // Adjust signal quantity based on inertia recommendation
                        let mut adjusted_signal = signal.clone();
                        let recommended_shares = decision.recommended_position / current_price;
                        adjusted_signal.quantity = recommended_shares.abs();
                        adjusted_signal.action = if recommended_shares >= 0.0 {
                            "BUY".to_string()
                        } else {
                            "SELL".to_string()
                        };

                        debug!(
                            "Position inertia allows signal for {}: {} -> {} (recommended: {})",
                            signal.symbol,
                            current_position,
                            target_position,
                            decision.recommended_position
                        );

                        filtered_signals.push(adjusted_signal);
                    }
                    InertiaDecision::Hold => {
                        inertia_filtered_count += 1;
                        debug!(
                            "Position inertia blocks signal for {}: {} (reason: {})",
                            signal.symbol, target_position, decision.reason
                        );
                    }
                }
            }

            signals = filtered_signals;
            drop(inertia_calculator);
        }

        // Step 2: Apply transaction cost filtering
        if self.enable_transaction_cost_optimization {
            let mut cost_filtered_signals = Vec::new();

            for signal in signals {
                let price = latest_prices.get(&signal.symbol).unwrap_or(&signal.price);
                let estimated_cost = self.estimate_transaction_cost(&signal, price).await?;

                // Calculate cost in basis points
                let position_value = signal.quantity.abs() * price;
                let cost_bps = if position_value > 0.0 {
                    (estimated_cost / position_value) * 10000.0
                } else {
                    0.0
                };

                total_estimated_costs += estimated_cost;

                if cost_bps <= max_acceptable_cost_bps {
                    cost_filtered_signals.push(signal);
                } else {
                    cost_filtered_count += 1;
                }
            }

            signals = cost_filtered_signals;
        }

        let final_count = signals.len();

        let filter_result = SignalFilterResult {
            original_signals: original_count,
            inertia_filtered: inertia_filtered_count,
            cost_filtered: cost_filtered_count,
            final_signals: final_count,
            total_estimated_costs,
        };

        info!(
            "Signal filtering complete: {} -> {} signals (inertia filtered: {}, cost filtered: {}, total estimated costs: ${:.2})",
            original_count,
            final_count,
            inertia_filtered_count,
            cost_filtered_count,
            total_estimated_costs
        );

        Ok((signals, filter_result))
    }

    /// Estimate transaction cost for a signal
    pub async fn estimate_transaction_cost(
        &self,
        signal: &OrderSignal,
        current_price: &f64,
    ) -> Result<f64> {
        let cost_calculator = self.transaction_cost_calculator.lock().await;

        let security_type = match signal.symbol.contains("USD") || signal.symbol.contains(".") {
            true => SecurityType::Forex,
            false => SecurityType::Stock, // Default to stock for now
        };

        let daily_volume = 1_000_000.0; // Default daily volume - should be fetched from market data

        let total_cost = cost_calculator.calculate_total_cost(
            &signal.symbol,
            &security_type,
            signal.quantity.abs(),
            *current_price,
            daily_volume,
        )?;

        Ok(total_cost)
    }

    /// Validate final order before execution
    pub async fn validate_order_cost(
        &self,
        signal: &OrderSignal,
        current_price: f64,
        max_acceptable_cost_bps: f64,
    ) -> Result<bool> {
        if !self.enable_transaction_cost_optimization {
            return Ok(true);
        }

        let estimated_cost = self
            .estimate_transaction_cost(signal, &current_price)
            .await?;
        let position_value = signal.quantity.abs() * current_price;
        let cost_bps = if position_value > 0.0 {
            (estimated_cost / position_value) * 10000.0
        } else {
            0.0
        };

        let is_acceptable = cost_bps <= max_acceptable_cost_bps;

        if !is_acceptable {
            warn!(
                "Final order cost validation failed for {}: {:.2} bps > {:.2} bps",
                signal.symbol, cost_bps, max_acceptable_cost_bps
            );
        }

        Ok(is_acceptable)
    }

    /// Update bid-ask spreads from market data
    pub async fn update_spread_for_symbol(&self, symbol: &str, spread: f64) -> Result<()> {
        let _cost_calculator = self.transaction_cost_calculator.lock().await;
        // Note: This would require adding an update method to TransactionCostCalculator
        // For now, we'll log the update
        debug!("Would update spread for {}: {:.4}%", symbol, spread * 100.0);
        Ok(())
    }

    /// Get current inertia configuration
    pub async fn get_inertia_config(&self) -> Result<InertiaConfig> {
        let _inertia_calculator = self.position_inertia_calculator.lock().await;
        // Note: This would require adding a get_config method to PositionInertiaCalculator
        // For now, return a default config
        Ok(InertiaConfig {
            inertia_multiplier: 2.0,
            min_position_change_value: 100.0,
            max_position_change_pct: 0.50,
            enable_position_inertia: true,
        })
    }
}
