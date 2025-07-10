use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ERCInertiaDecision {
    Hold,
    Rebalance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ERCInertiaDecisionInfo {
    pub symbol: String,
    pub current_allocation: f64,
    pub target_allocation: f64,
    pub recommended_allocation: f64,
    pub action: ERCInertiaDecision,
    pub transaction_cost: f64,
    pub position_change_value: f64,
    pub inertia_threshold: f64,
    pub blocked_by_inertia: bool,
    pub correlation_risk_boost: f64,
    pub volatility_adjustment_applied: bool,
    pub target_allocation_adjusted: bool,
    pub reason: String,
}

pub struct RiskBudgetingInertiaCalculator {
    inertia_multiplier: f64,
    rebalance_threshold: f64,
    enable_cost_aware_erc: bool,
}

impl RiskBudgetingInertiaCalculator {
    pub fn new(
        inertia_multiplier: f64,
        rebalance_threshold: f64,
        enable_cost_aware_erc: bool,
    ) -> Self {
        Self {
            inertia_multiplier,
            rebalance_threshold,
            enable_cost_aware_erc,
        }
    }

    pub fn calculate_erc_with_inertia(
        &self,
        current_positions: Vec<(&str, f64, f64)>, // (symbol, value, weight)
        erc_targets: Vec<(&str, f64)>,            // (symbol, target_weight)
        transaction_costs: Vec<(&str, f64)>,      // (symbol, cost)
        total_portfolio_value: f64,
    ) -> Result<Vec<ERCInertiaDecisionInfo>> {
        let mut decisions = Vec::new();

        for (i, (symbol, current_value, current_weight)) in current_positions.iter().enumerate() {
            let target_weight = erc_targets[i].1;
            let transaction_cost = transaction_costs[i].1;

            let target_value = target_weight * total_portfolio_value;
            let position_change_value = (target_value - current_value).abs();
            let inertia_threshold = transaction_cost * self.inertia_multiplier;

            let position_drift_pct = ((target_weight - current_weight) / current_weight).abs();
            let is_large_drift = position_drift_pct > self.rebalance_threshold + f64::EPSILON;

            let action = if !self.enable_cost_aware_erc {
                ERCInertiaDecision::Rebalance
            } else if is_large_drift {
                ERCInertiaDecision::Rebalance // Large drift overrides cost considerations
            } else {
                // For small drifts, always hold to reduce transaction costs
                ERCInertiaDecision::Hold
            };

            let blocked_by_inertia =
                self.enable_cost_aware_erc && action == ERCInertiaDecision::Hold;

            let reason = if !self.enable_cost_aware_erc {
                "Cost-aware ERC disabled".to_string()
            } else if is_large_drift {
                "Large position drift exceeds threshold".to_string()
            } else if position_change_value > inertia_threshold {
                "Position change exceeds inertia threshold".to_string()
            } else {
                "Position change blocked by inertia".to_string()
            };

            decisions.push(ERCInertiaDecisionInfo {
                symbol: symbol.to_string(),
                current_allocation: *current_weight,
                target_allocation: target_weight,
                recommended_allocation: if action == ERCInertiaDecision::Rebalance {
                    target_weight
                } else {
                    *current_weight
                },
                action: action.clone(),
                transaction_cost,
                position_change_value,
                inertia_threshold,
                blocked_by_inertia,
                correlation_risk_boost: 0.0,
                volatility_adjustment_applied: false,
                target_allocation_adjusted: action == ERCInertiaDecision::Rebalance,
                reason,
            });
        }

        Ok(decisions)
    }

    pub fn calculate_erc_with_correlation_risk(
        &self,
        current_positions: Vec<(&str, f64, f64)>,
        erc_targets: Vec<(&str, f64)>,
        correlation_matrix: Vec<Vec<f64>>,
        transaction_costs: Vec<(&str, f64)>,
        total_portfolio_value: f64,
    ) -> Result<Vec<ERCInertiaDecisionInfo>> {
        let mut decisions = self.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            total_portfolio_value,
        )?;

        // Apply correlation risk boost
        for (i, decision) in decisions.iter_mut().enumerate() {
            let avg_correlation =
                correlation_matrix[i].iter().sum::<f64>() / correlation_matrix[i].len() as f64;
            decision.correlation_risk_boost = (avg_correlation - 0.5).max(0.0) * 0.1;
        }

        Ok(decisions)
    }

    pub fn calculate_erc_with_volatility_adjustment(
        &self,
        current_positions: Vec<(&str, f64, f64)>,
        erc_targets: Vec<(&str, f64)>,
        transaction_costs: Vec<(&str, f64)>,
        portfolio_volatility: f64,
        target_volatility: f64,
        total_portfolio_value: f64,
    ) -> Result<Vec<ERCInertiaDecisionInfo>> {
        let mut decisions = self.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            total_portfolio_value,
        )?;

        // If portfolio volatility exceeds target, override inertia
        if portfolio_volatility > target_volatility {
            for decision in decisions.iter_mut() {
                decision.volatility_adjustment_applied = true;
                if decision.action == ERCInertiaDecision::Hold {
                    decision.action = ERCInertiaDecision::Rebalance;
                    decision.reason = "Volatility adjustment overrides inertia".to_string();
                }
            }
        }

        Ok(decisions)
    }
}
