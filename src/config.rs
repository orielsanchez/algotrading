use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use crate::security_types::SecurityType;
use crate::futures_utils::get_front_month_contract;
use log::{info, warn};

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
}

fn default_max_margin_utilization() -> f64 { 0.70 }
fn default_min_excess_liquidity() -> f64 { 10000.0 }
fn default_futures_position_limit() -> f64 { 10.0 }
fn default_margin_call_threshold() -> f64 { 0.85 }
fn default_margin_buffer_percentage() -> f64 { 0.20 }

impl TradingConfig {
    pub fn load() -> Result<Self> {
        let config_str = fs::read_to_string("config.json")
            .unwrap_or_else(|_| Self::default_config_json());
        
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
            },
        }
    }
}