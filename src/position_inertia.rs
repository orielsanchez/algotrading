use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InertiaConfig {
    pub inertia_multiplier: f64,
    pub min_position_change_value: f64,
    pub max_position_change_pct: f64,
    pub enable_position_inertia: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InertiaDecision {
    Hold,
    Rebalance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InertiaDecisionInfo {
    pub action: InertiaDecision,
    pub recommended_position: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBenefitAnalysis {
    pub position_change_value: f64,
    pub transaction_cost: f64,
    pub inertia_threshold: f64,
    pub net_benefit: f64,
    pub is_beneficial: bool,
    pub exceeds_threshold: bool,
}

pub struct PositionInertiaCalculator {
    config: InertiaConfig,
}

impl PositionInertiaCalculator {
    pub fn new(config: InertiaConfig) -> Self {
        Self { config }
    }

    pub fn calculate_inertia_threshold(&self, transaction_cost: f64) -> Result<f64> {
        Ok(transaction_cost * self.config.inertia_multiplier)
    }

    pub fn calculate_position_decision(
        &self,
        current_position: f64,
        target_position: f64,
        transaction_cost: f64,
        signal_strength: f64,
        _price: f64,
    ) -> Result<InertiaDecisionInfo> {
        // If inertia is disabled, always rebalance
        if !self.config.enable_position_inertia {
            return Ok(InertiaDecisionInfo {
                action: InertiaDecision::Rebalance,
                recommended_position: target_position,
                reason: "Position inertia disabled".to_string(),
            });
        }

        let position_change_value = (target_position - current_position).abs();
        let inertia_threshold = self.calculate_inertia_threshold(transaction_cost)?;

        // Check minimum position change threshold
        if position_change_value < self.config.min_position_change_value {
            return Ok(InertiaDecisionInfo {
                action: InertiaDecision::Hold,
                recommended_position: current_position,
                reason: "Position change below minimum change threshold".to_string(),
            });
        }

        // Check for position reversal (always execute)
        if (current_position > 0.0 && target_position < 0.0)
            || (current_position < 0.0 && target_position > 0.0)
        {
            return Ok(InertiaDecisionInfo {
                action: InertiaDecision::Rebalance,
                recommended_position: target_position,
                reason: "Position reversal detected".to_string(),
            });
        }

        // Check for very strong signal (overrides inertia)
        if signal_strength.abs() >= 18.0 {
            // Apply maximum position change limiting even for strong signals
            let max_change_value = current_position.abs() * self.config.max_position_change_pct;

            let recommended_position = if target_position > current_position {
                let change_amount = (target_position - current_position).min(max_change_value);
                current_position + change_amount
            } else {
                let change_amount = (current_position - target_position).min(max_change_value);
                current_position - change_amount
            };

            let reason = if (recommended_position - target_position).abs() > 0.01 {
                "Strong signal overrides inertia, position change limited to maximum percentage".to_string()
            } else {
                "Strong signal overrides inertia".to_string()
            };

            return Ok(InertiaDecisionInfo {
                action: InertiaDecision::Rebalance,
                recommended_position,
                reason,
            });
        }

        // Check inertia threshold
        if position_change_value > inertia_threshold {
            // Apply maximum position change limiting
            let max_change_value = current_position.abs() * self.config.max_position_change_pct;

            let recommended_position = if target_position > current_position {
                let change_amount = (target_position - current_position).min(max_change_value);
                current_position + change_amount
            } else {
                let change_amount = (current_position - target_position).min(max_change_value);
                current_position - change_amount
            };

            let reason = if (recommended_position - target_position).abs() > 0.01 {
                "Position change limited to maximum percentage".to_string()
            } else {
                "Position change threshold exceeded".to_string()
            };

            return Ok(InertiaDecisionInfo {
                action: InertiaDecision::Rebalance,
                recommended_position,
                reason,
            });
        }

        // Hold position due to inertia
        Ok(InertiaDecisionInfo {
            action: InertiaDecision::Hold,
            recommended_position: current_position,
            reason: "Position change blocked by inertia".to_string(),
        })
    }

    pub fn analyze_portfolio_rebalancing(
        &self,
        positions: Vec<(&str, f64, f64, f64)>, // (symbol, current, target, tx_cost)
    ) -> Result<Vec<InertiaDecisionInfo>> {
        let mut decisions = Vec::new();

        for (_symbol, current_position, target_position, transaction_cost) in positions {
            let decision = self.calculate_position_decision(
                current_position,
                target_position,
                transaction_cost,
                10.0,  // Default signal strength
                100.0, // Default price
            )?;
            decisions.push(decision);
        }

        Ok(decisions)
    }

    pub fn calculate_cost_benefit_analysis(
        &self,
        current_position: f64,
        target_position: f64,
        transaction_cost: f64,
        _signal_strength: f64,
        _price: f64,
    ) -> Result<CostBenefitAnalysis> {
        let position_change_value = (target_position - current_position).abs();
        let inertia_threshold = self.calculate_inertia_threshold(transaction_cost)?;
        let net_benefit = position_change_value - transaction_cost;

        Ok(CostBenefitAnalysis {
            position_change_value,
            transaction_cost,
            inertia_threshold,
            net_benefit,
            is_beneficial: net_benefit > 0.0,
            exceeds_threshold: position_change_value > inertia_threshold,
        })
    }
}
