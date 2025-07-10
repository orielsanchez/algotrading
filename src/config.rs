use crate::futures_utils::get_front_month_contract;
use crate::security_types::SecurityType;
use anyhow::Result;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub tws_config: TwsConfig,
    pub strategy_config: StrategyConfig,
    pub risk_config: RiskConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwsConfig {
    pub host: String,
    pub port: u16,
    pub client_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub securities: Vec<SecurityConfig>,
    pub lookback_period: usize,
    pub momentum_threshold: f64,
    pub position_size: f64,
    pub rebalance_frequency_minutes: u64,
    #[serde(default = "default_target_volatility")]
    pub target_volatility: f64,
    #[serde(default = "default_volatility_halflife")]
    pub volatility_halflife: f64,
    #[serde(default = "default_use_limit_orders")]
    pub use_limit_orders: bool,
    #[serde(default = "default_limit_order_offset")]
    pub limit_order_offset: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub symbol: String,
    #[serde(rename = "type")]
    pub security_type: SecurityType,
    pub exchange: String,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub futures_specs: Option<FuturesSpecs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesSpecs {
    pub underlying: String,
    pub expiry: String,
    pub multiplier: f64,
    pub tick_size: f64,
    pub contract_month: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub max_position_size: f64,
    pub max_portfolio_exposure: f64,
    pub stop_loss_percentage: f64,
    pub take_profit_percentage: f64,
    #[serde(default = "default_max_margin_utilization")]
    pub max_margin_utilization: f64,
    #[serde(default = "default_min_excess_liquidity")]
    pub min_excess_liquidity: f64,
    #[serde(default = "default_futures_position_limit")]
    pub futures_position_limit: f64,
    #[serde(default = "default_margin_call_threshold")]
    pub margin_call_threshold: f64,
    #[serde(default = "default_margin_buffer_percentage")]
    pub margin_buffer_percentage: f64,
    // Risk Budgeting Configuration
    #[serde(default = "default_enable_risk_budgeting")]
    pub enable_risk_budgeting: bool,
    #[serde(default = "default_risk_budget_target_volatility")]
    pub risk_budget_target_volatility: f64,
    #[serde(default = "default_risk_budget_rebalance_threshold")]
    pub risk_budget_rebalance_threshold: f64,
    #[serde(default = "default_max_correlation_exposure")]
    pub max_correlation_exposure: f64,
    #[serde(default = "default_correlation_lookback_days")]
    pub correlation_lookback_days: usize,
    #[serde(default = "default_min_positions_for_erc")]
    pub min_positions_for_erc: usize,
    // Transaction Cost Configuration
    #[serde(default = "default_enable_transaction_cost_optimization")]
    pub enable_transaction_cost_optimization: bool,
    #[serde(default = "default_stock_commission")]
    pub stock_commission: f64,
    #[serde(default = "default_futures_commission")]
    pub futures_commission: f64,
    #[serde(default = "default_forex_commission")]
    pub forex_commission: f64,
    #[serde(default = "default_max_acceptable_cost_bps")]
    pub max_acceptable_cost_bps: f64,
    // Position Inertia Configuration
    #[serde(default = "default_enable_position_inertia")]
    pub enable_position_inertia: bool,
    #[serde(default = "default_inertia_multiplier")]
    pub inertia_multiplier: f64,
    #[serde(default = "default_min_position_change_value")]
    pub min_position_change_value: f64,
    #[serde(default = "default_max_position_change_pct")]
    pub max_position_change_pct: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_size: 0.5,  // 50% of portfolio per position
            max_portfolio_exposure: 1.0,  // 100% max exposure
            stop_loss_percentage: 0.02,  // 2% stop loss
            take_profit_percentage: 0.04,  // 4% take profit
            max_margin_utilization: default_max_margin_utilization(),
            min_excess_liquidity: default_min_excess_liquidity(),
            futures_position_limit: default_futures_position_limit(),
            margin_call_threshold: default_margin_call_threshold(),
            margin_buffer_percentage: default_margin_buffer_percentage(),
            enable_risk_budgeting: default_enable_risk_budgeting(),
            risk_budget_target_volatility: default_risk_budget_target_volatility(),
            risk_budget_rebalance_threshold: default_risk_budget_rebalance_threshold(),
            max_correlation_exposure: default_max_correlation_exposure(),
            correlation_lookback_days: default_correlation_lookback_days(),
            min_positions_for_erc: default_min_positions_for_erc(),
            enable_transaction_cost_optimization: default_enable_transaction_cost_optimization(),
            stock_commission: default_stock_commission(),
            futures_commission: default_futures_commission(),
            forex_commission: default_forex_commission(),
            max_acceptable_cost_bps: default_max_acceptable_cost_bps(),
            enable_position_inertia: default_enable_position_inertia(),
            inertia_multiplier: default_inertia_multiplier(),
            min_position_change_value: default_min_position_change_value(),
            max_position_change_pct: default_max_position_change_pct(),
        }
    }
}

fn default_max_margin_utilization() -> f64 {
    0.70
}
fn default_min_excess_liquidity() -> f64 {
    10000.0
}
fn default_futures_position_limit() -> f64 {
    10.0
}
fn default_margin_call_threshold() -> f64 {
    0.85
}
fn default_margin_buffer_percentage() -> f64 {
    0.20
}

fn default_target_volatility() -> f64 {
    0.25 // 25% annualized target volatility
}

fn default_volatility_halflife() -> f64 {
    32.0 // 32-day half-life for EWMA
}

fn default_use_limit_orders() -> bool {
    true // Prefer limit orders for better execution prices
}

fn default_limit_order_offset() -> f64 {
    0.01 // 1% offset from current price for limit orders
}

// Risk Budgeting Configuration Defaults
fn default_enable_risk_budgeting() -> bool {
    true // Enable risk budgeting by default
}

fn default_risk_budget_target_volatility() -> f64 {
    0.15 // 15% target portfolio volatility for risk budgeting
}

fn default_risk_budget_rebalance_threshold() -> f64 {
    0.05 // 5% deviation from target risk allocation triggers rebalancing
}

fn default_max_correlation_exposure() -> f64 {
    0.60 // Maximum 60% exposure to highly correlated assets
}

fn default_correlation_lookback_days() -> usize {
    63 // ~3 months of trading days for correlation calculation
}

fn default_min_positions_for_erc() -> usize {
    3 // Minimum 3 positions required for Equal Risk Contribution
}

// Transaction Cost Configuration Defaults
fn default_enable_transaction_cost_optimization() -> bool {
    true // Enable transaction cost optimization by default
}

fn default_stock_commission() -> f64 {
    1.00 // $1.00 per stock trade
}

fn default_futures_commission() -> f64 {
    2.50 // $2.50 per futures contract
}

fn default_forex_commission() -> f64 {
    0.50 // $0.50 per forex trade
}

fn default_max_acceptable_cost_bps() -> f64 {
    15.0 // 15 basis points maximum acceptable transaction cost
}

// Position Inertia Configuration Defaults
fn default_enable_position_inertia() -> bool {
    true // Enable position inertia by default (Carver framework)
}

fn default_inertia_multiplier() -> f64 {
    2.0 // Carver's 2x transaction cost inertia threshold
}

fn default_min_position_change_value() -> f64 {
    100.0 // $100 minimum position change to execute
}

fn default_max_position_change_pct() -> f64 {
    0.50 // 50% maximum position change per rebalance
}

impl TradingConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_file("config.json")
    }

    pub fn load_from_file(path: &str) -> Result<Self> {
        let config_str = fs::read_to_string(path).unwrap_or_else(|_| Self::default_config_json());

        let mut config: TradingConfig = serde_json::from_str(&config_str)?;

        // Update futures contracts with current expiry dates
        config.update_futures_expiries()?;

        Ok(config)
    }

    fn default_config_json() -> String {
        serde_json::to_string_pretty(&Self::default()).unwrap()
    }

    /// Update futures contracts with current front-month expiry dates
    fn update_futures_expiries(&mut self) -> Result<()> {
        for security in &mut self.strategy_config.securities {
            if security.security_type == SecurityType::Future {
                if let Some(futures_specs) = &mut security.futures_specs {
                    match get_front_month_contract(&security.symbol) {
                        Ok((expiry, contract_month)) => {
                            info!(
                                "Updating {} futures contract: expiry {} -> {}, month {} -> {}",
                                security.symbol,
                                futures_specs.expiry,
                                expiry,
                                futures_specs.contract_month,
                                contract_month
                            );
                            futures_specs.expiry = expiry;
                            futures_specs.contract_month = contract_month;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to update expiry for {}: {}. Using existing dates.",
                                security.symbol, e
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            tws_config: TwsConfig {
                host: "127.0.0.1".to_string(),
                port: 7497,
                client_id: 1,
            },
            strategy_config: StrategyConfig {
                securities: vec![
                    SecurityConfig {
                        symbol: "AAPL".to_string(),
                        security_type: SecurityType::Stock,
                        exchange: "SMART".to_string(),
                        currency: "USD".to_string(),
                        futures_specs: None,
                    },
                    SecurityConfig {
                        symbol: "MSFT".to_string(),
                        security_type: SecurityType::Stock,
                        exchange: "SMART".to_string(),
                        currency: "USD".to_string(),
                        futures_specs: None,
                    },
                    SecurityConfig {
                        symbol: "ES".to_string(),
                        security_type: SecurityType::Future,
                        exchange: "CME".to_string(),
                        currency: "USD".to_string(),
                        futures_specs: Some(FuturesSpecs {
                            underlying: "ES".to_string(),
                            expiry: "20240315".to_string(),
                            multiplier: 50.0,
                            tick_size: 0.25,
                            contract_month: "202403".to_string(),
                        }),
                    },
                ],
                lookback_period: 20,
                momentum_threshold: 0.02,
                position_size: 10000.0,
                rebalance_frequency_minutes: 60,
                target_volatility: default_target_volatility(),
                volatility_halflife: default_volatility_halflife(),
                use_limit_orders: default_use_limit_orders(),
                limit_order_offset: default_limit_order_offset(),
            },
            risk_config: RiskConfig {
                max_position_size: 50000.0,
                max_portfolio_exposure: 0.95,
                stop_loss_percentage: 0.02,
                take_profit_percentage: 0.05,
                max_margin_utilization: 0.70,
                min_excess_liquidity: 10000.0,
                futures_position_limit: 10.0,
                margin_call_threshold: 0.85,
                margin_buffer_percentage: 0.20,
                enable_risk_budgeting: default_enable_risk_budgeting(),
                risk_budget_target_volatility: default_risk_budget_target_volatility(),
                risk_budget_rebalance_threshold: default_risk_budget_rebalance_threshold(),
                max_correlation_exposure: default_max_correlation_exposure(),
                correlation_lookback_days: default_correlation_lookback_days(),
                min_positions_for_erc: default_min_positions_for_erc(),
                enable_transaction_cost_optimization: default_enable_transaction_cost_optimization(),
                stock_commission: default_stock_commission(),
                futures_commission: default_futures_commission(),
                forex_commission: default_forex_commission(),
                max_acceptable_cost_bps: default_max_acceptable_cost_bps(),
                enable_position_inertia: default_enable_position_inertia(),
                inertia_multiplier: default_inertia_multiplier(),
                min_position_change_value: default_min_position_change_value(),
                max_position_change_pct: default_max_position_change_pct(),
            },
        }
    }
}
