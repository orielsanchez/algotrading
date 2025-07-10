use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::security_types::SecurityType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCostConfig {
    pub bid_ask_spreads: HashMap<String, f64>,
    pub commission_rates: HashMap<SecurityType, f64>,
    pub market_impact_threshold: f64,
    pub market_impact_coefficient: f64,
}

#[derive(Debug, Clone)]
pub struct MarketImpactModel {
    pub threshold: f64,
    pub coefficient: f64,
}

pub struct TransactionCostCalculator {
    config: TransactionCostConfig,
}

impl TransactionCostCalculator {
    pub fn new(config: TransactionCostConfig) -> Self {
        Self { config }
    }

    pub fn calculate_spread_cost(
        &self,
        symbol: &str,
        security_type: &SecurityType,
        quantity: f64,
        price: f64,
    ) -> Result<f64> {
        let spread_rate = self.config.bid_ask_spreads
            .get(symbol)
            .copied()
            .unwrap_or(0.0010); // Default 0.10% spread

        let position_value = price * quantity.abs();
        Ok(position_value * spread_rate)
    }

    pub fn calculate_commission_cost(
        &self,
        security_type: &SecurityType,
        quantity: f64,
    ) -> Result<f64> {
        let commission_rate = self.config.commission_rates
            .get(&security_type)
            .copied()
            .unwrap_or(1.00); // Default $1.00

        match security_type {
            SecurityType::Stock => Ok(commission_rate),
            SecurityType::Forex => Ok(commission_rate),
            SecurityType::Future => Ok(commission_rate * quantity.abs()),
        }
    }

    pub fn calculate_market_impact_cost(
        &self,
        symbol: &str,
        quantity: f64,
        price: f64,
        daily_volume: f64,
    ) -> Result<f64> {
        let position_value = price * quantity.abs();
        let volume_percentage = position_value / daily_volume;

        if volume_percentage <= self.config.market_impact_threshold {
            return Ok(0.0);
        }

        let excess_percentage = volume_percentage - self.config.market_impact_threshold;
        let impact_cost = position_value * excess_percentage * self.config.market_impact_coefficient;
        
        Ok(impact_cost)
    }

    pub fn calculate_total_cost(
        &self,
        symbol: &str,
        security_type: &SecurityType,
        quantity: f64,
        price: f64,
        daily_volume: f64,
    ) -> Result<f64> {
        if quantity == 0.0 {
            return Ok(0.0);
        }

        let spread_cost = self.calculate_spread_cost(symbol, security_type, quantity, price)?;
        let commission_cost = self.calculate_commission_cost(security_type, quantity)?;
        let market_impact_cost = self.calculate_market_impact_cost(symbol, quantity, price, daily_volume)?;

        Ok(spread_cost + commission_cost + market_impact_cost)
    }

    pub fn calculate_round_trip_cost(
        &self,
        symbol: &str,
        security_type: &SecurityType,
        quantity: f64,
        price: f64,
        daily_volume: f64,
    ) -> Result<f64> {
        let one_way_cost = self.calculate_total_cost(symbol, security_type, quantity, price, daily_volume)?;
        Ok(one_way_cost * 2.0)
    }
}