use anyhow::{Result, anyhow};
use log::info;
use std::collections::HashMap;

use crate::orders::OrderSignal;
use crate::portfolio::Portfolio;
use crate::security_types::{SecurityInfo, SecurityType};

#[derive(Debug, Clone)]
pub struct MarginInfo {
    pub initial_margin: f64,
    pub maintenance_margin: f64,
    pub margin_requirement_percentage: f64,
}

#[derive(Debug, Clone)]
pub struct MarginValidation {
    pub has_sufficient_margin: bool,
    pub required_margin: f64,
    pub available_margin: f64,
    pub margin_utilization_after: f64,
    pub excess_liquidity_after: f64,
    pub warning_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarginStatus {
    Healthy,
    Warning(String),
    Critical(String),
}

#[derive(Debug, Clone)]
pub struct MarginRequirements {
    pub initial_percentage: f64,
    pub maintenance_percentage: f64,
}

/// Get margin requirements for different futures contracts
/// Returns (initial_margin_percentage, maintenance_margin_percentage)
pub fn get_futures_margin_requirements(symbol: &str) -> MarginRequirements {
    match symbol {
        "ES" => MarginRequirements {
            initial_percentage: 0.12,     // 12% initial margin
            maintenance_percentage: 0.11, // 11% maintenance margin
        },
        "NQ" => MarginRequirements {
            initial_percentage: 0.13,     // 13% initial margin
            maintenance_percentage: 0.12, // 12% maintenance margin
        },
        "CL" => MarginRequirements {
            initial_percentage: 0.15, // 15% for crude oil
            maintenance_percentage: 0.13,
        },
        "GC" => MarginRequirements {
            initial_percentage: 0.08, // 8% for gold
            maintenance_percentage: 0.07,
        },
        "ZB" => MarginRequirements {
            initial_percentage: 0.03, // 3% for bonds
            maintenance_percentage: 0.025,
        },
        _ => MarginRequirements {
            // Conservative default
            initial_percentage: 0.20,
            maintenance_percentage: 0.15,
        },
    }
}

/// Calculate initial margin requirement for a position
pub fn calculate_initial_margin(
    security_info: &SecurityInfo,
    quantity: f64,
    price: f64,
) -> Result<f64> {
    match security_info.security_type {
        SecurityType::Stock => {
            // For stocks, typically 25% margin requirement (4:1 leverage)
            Ok(quantity * price * 0.25)
        }
        SecurityType::Future => {
            let contract = security_info
                .contract_specs
                .as_ref()
                .ok_or_else(|| anyhow!("Missing futures contract info"))?;

            let margin_req = get_futures_margin_requirements(&security_info.symbol);
            let contract_value = quantity * price * contract.multiplier;
            let margin = contract_value * margin_req.initial_percentage;

            info!(
                "Initial margin for {} contracts of {}: ${:.2} ({}% of ${:.2} value)",
                quantity,
                security_info.symbol,
                margin,
                margin_req.initial_percentage * 100.0,
                contract_value
            );

            Ok(margin)
        }
        SecurityType::Forex => {
            // For forex, typically 2% margin requirement (50:1 leverage)
            Ok(quantity * price * 0.02)
        }
    }
}

/// Calculate maintenance margin requirement for a position
pub fn calculate_maintenance_margin(
    security_info: &SecurityInfo,
    quantity: f64,
    price: f64,
) -> Result<f64> {
    match security_info.security_type {
        SecurityType::Stock => {
            // For stocks, typically 25% maintenance requirement
            Ok(quantity * price * 0.25)
        }
        SecurityType::Future => {
            let contract = security_info
                .contract_specs
                .as_ref()
                .ok_or_else(|| anyhow!("Missing futures contract info"))?;

            let margin_req = get_futures_margin_requirements(&security_info.symbol);
            let contract_value = quantity * price * contract.multiplier;
            let margin = contract_value * margin_req.maintenance_percentage;

            Ok(margin)
        }
        SecurityType::Forex => {
            // For forex, typically 2% maintenance requirement
            Ok(quantity * price * 0.02)
        }
    }
}

/// Validate margin requirements before placing an order
pub fn validate_margin_requirements(
    _portfolio: &Portfolio,
    order: &OrderSignal,
    account_summary: &HashMap<String, f64>,
    max_margin_utilization: f64,
) -> Result<MarginValidation> {
    // Get account values
    let available_funds = account_summary
        .get("available_funds")
        .copied()
        .unwrap_or(0.0);
    let initial_margin_req = account_summary
        .get("initial_margin")
        .copied()
        .unwrap_or(0.0);
    let _maintenance_margin_req = account_summary
        .get("maintenance_margin")
        .copied()
        .unwrap_or(0.0);
    let net_liquidation = account_summary
        .get("net_liquidation")
        .copied()
        .unwrap_or(0.0);

    // Calculate margin for the new position
    let new_position_margin =
        calculate_initial_margin(&order.security_info, order.quantity, order.price)?;

    // Calculate total margin after this trade
    let total_margin_after = initial_margin_req + new_position_margin;
    let margin_utilization_after = if net_liquidation > 0.0 {
        total_margin_after / net_liquidation
    } else {
        1.0
    };

    // Calculate excess liquidity after trade
    let excess_liquidity_after = available_funds - new_position_margin;

    // Validation checks
    let mut has_sufficient_margin = true;
    let mut warning_message = None;

    // Check 1: Do we have enough available funds?
    if new_position_margin > available_funds {
        has_sufficient_margin = false;
        warning_message = Some(format!(
            "Insufficient funds: need ${:.2} but only ${:.2} available",
            new_position_margin, available_funds
        ));
    }

    // Check 2: Will margin utilization exceed limit?
    if margin_utilization_after > max_margin_utilization {
        has_sufficient_margin = false;
        warning_message = Some(format!(
            "Margin utilization would exceed limit: {:.1}% > {:.1}%",
            margin_utilization_after * 100.0,
            max_margin_utilization * 100.0
        ));
    }

    // Check 3: Warning if getting close to limit
    if margin_utilization_after > max_margin_utilization * 0.9 && has_sufficient_margin {
        warning_message = Some(format!(
            "Warning: Margin utilization approaching limit: {:.1}%",
            margin_utilization_after * 100.0
        ));
    }

    // Check 4: Minimum excess liquidity
    let min_excess_liquidity = 10000.0; // Hardcoded for now, should come from config
    if excess_liquidity_after < min_excess_liquidity && has_sufficient_margin {
        warning_message = Some(format!(
            "Warning: Low excess liquidity after trade: ${:.2}",
            excess_liquidity_after
        ));
    }

    Ok(MarginValidation {
        has_sufficient_margin,
        required_margin: new_position_margin,
        available_margin: available_funds,
        margin_utilization_after,
        excess_liquidity_after,
        warning_message,
    })
}

/// Check overall margin health of the portfolio
pub fn check_margin_health(
    _portfolio: &Portfolio,
    account_summary: &HashMap<String, f64>,
    margin_call_threshold: f64,
) -> MarginStatus {
    let maintenance_margin = account_summary
        .get("maintenance_margin")
        .copied()
        .unwrap_or(0.0);
    let net_liquidation = account_summary
        .get("net_liquidation")
        .copied()
        .unwrap_or(0.0);
    let excess_liquidity = account_summary
        .get("excess_liquidity")
        .copied()
        .unwrap_or({
            // Calculate if not provided
            net_liquidation - maintenance_margin
        });

    if net_liquidation == 0.0 {
        return MarginStatus::Warning(
            "Unable to determine margin status: zero net liquidation".to_string(),
        );
    }

    let margin_utilization = maintenance_margin / net_liquidation;
    let margin_cushion = excess_liquidity / net_liquidation;

    // Critical: Approaching margin call
    if margin_utilization > margin_call_threshold {
        return MarginStatus::Critical(format!(
            "MARGIN CALL RISK: Utilization {:.1}% exceeds threshold {:.1}%. Excess liquidity: ${:.2}",
            margin_utilization * 100.0,
            margin_call_threshold * 100.0,
            excess_liquidity
        ));
    }

    // Warning: Getting close to danger zone
    if margin_utilization > margin_call_threshold * 0.8 {
        return MarginStatus::Warning(format!(
            "High margin utilization: {:.1}%. Excess liquidity: ${:.2}",
            margin_utilization * 100.0,
            excess_liquidity
        ));
    }

    // Warning: Low cushion
    if margin_cushion < 0.15 {
        // Less than 15% cushion
        return MarginStatus::Warning(format!(
            "Low margin cushion: {:.1}%. Consider reducing positions",
            margin_cushion * 100.0
        ));
    }

    MarginStatus::Healthy
}

/// Calculate maximum position size based on available margin
pub fn calculate_max_position_size(
    available_margin: f64,
    security_info: &SecurityInfo,
    price: f64,
    max_utilization: f64,
) -> Result<f64> {
    let usable_margin = available_margin * max_utilization;

    match security_info.security_type {
        SecurityType::Stock => {
            // For stocks with 4:1 leverage (25% margin)
            Ok((usable_margin / (price * 0.25)).floor())
        }
        SecurityType::Future => {
            let contract = security_info
                .contract_specs
                .as_ref()
                .ok_or_else(|| anyhow!("Missing futures contract info"))?;

            let margin_req = get_futures_margin_requirements(&security_info.symbol);
            let margin_per_contract = price * contract.multiplier * margin_req.initial_percentage;

            if margin_per_contract > 0.0 {
                Ok((usable_margin / margin_per_contract).floor())
            } else {
                Ok(0.0)
            }
        }
        SecurityType::Forex => {
            // Forex typically has 50:1 leverage (2% margin)
            Ok((usable_margin / (price * 0.02)).floor())
        }
    }
}

/// Calculate portfolio-wide margin statistics
pub fn calculate_portfolio_margin_stats(
    positions: &HashMap<String, crate::portfolio::Position>,
    market_data: &HashMap<String, f64>,
) -> Result<(f64, f64)> {
    let mut total_initial_margin = 0.0;
    let mut total_maintenance_margin = 0.0;

    for (symbol, position) in positions {
        if let Some(&current_price) = market_data.get(symbol) {
            if let Some(ref security_info) = position.security_info {
                let initial =
                    calculate_initial_margin(security_info, position.quantity, current_price)?;
                let maintenance =
                    calculate_maintenance_margin(security_info, position.quantity, current_price)?;

                total_initial_margin += initial;
                total_maintenance_margin += maintenance;
            }
        }
    }

    Ok((total_initial_margin, total_maintenance_margin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security_types::FuturesContract;

    fn create_test_futures_info(symbol: &str, multiplier: f64) -> SecurityInfo {
        SecurityInfo {
            symbol: symbol.to_string(),
            security_type: SecurityType::Future,
            exchange: "CME".to_string(),
            currency: "USD".to_string(),
            contract_specs: Some(FuturesContract {
                underlying: symbol.to_string(),
                expiry: "20250321".to_string(),
                multiplier,
                tick_size: 0.25,
                contract_month: "202503".to_string(),
            }),
            forex_pair: None,
        }
    }

    #[test]
    fn test_futures_margin_calculation() {
        let es_info = create_test_futures_info("ES", 50.0);
        let price = 5500.0;
        let quantity = 2.0;

        let initial_margin = calculate_initial_margin(&es_info, quantity, price).unwrap();
        let contract_value = quantity * price * 50.0; // 550,000
        let expected_margin = contract_value * 0.12; // 12% for ES

        assert!((initial_margin - expected_margin).abs() < 0.01);
    }

    #[test]
    fn test_margin_validation() {
        let es_info = create_test_futures_info("ES", 50.0);
        let order = OrderSignal {
            symbol: "ES".to_string(),
            action: "BUY".to_string(),
            quantity: 1.0,
            price: 5500.0,
            order_type: "MKT".to_string(),
            limit_price: None,
            reason: "Test order".to_string(),
            security_info: es_info,
        };

        let mut account_summary = HashMap::new();
        account_summary.insert("available_funds".to_string(), 50000.0);
        account_summary.insert("initial_margin".to_string(), 20000.0);
        account_summary.insert("net_liquidation".to_string(), 100000.0);

        let portfolio = crate::portfolio::Portfolio::new(100000.0);
        let validation = validate_margin_requirements(
            &portfolio,
            &order,
            &account_summary,
            0.70, // 70% max utilization
        )
        .unwrap();

        assert!(validation.has_sufficient_margin);
        assert!(validation.margin_utilization_after < 0.70);
    }
}
