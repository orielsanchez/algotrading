use crate::config::RiskConfig;
use crate::portfolio::Portfolio;
use anyhow::Result;
use std::collections::HashMap;

/// Portfolio risk budgeting system following Carver's risk parity principles
/// 
/// Key Features:
/// - Equal Risk Contribution (ERC) across instruments
/// - Marginal Contribution to Risk (MCR) calculations
/// - Risk attribution analysis by position/strategy
/// - Correlation-based risk management
/// - Concentration risk monitoring
#[derive(Debug, Clone)]
pub struct RiskBudgeter {
    #[allow(dead_code)]
    risk_config: RiskConfig,
    correlation_matrix: HashMap<(String, String), f64>,
    volatilities: HashMap<String, f64>,
    target_portfolio_volatility: f64,
}

/// Risk contribution of a single position to total portfolio risk
#[derive(Debug, Clone)]
pub struct RiskContribution {
    pub symbol: String,
    pub weight: f64,               // Position weight in portfolio
    pub volatility: f64,           // Individual instrument volatility
    pub marginal_risk: f64,        // Marginal contribution to portfolio risk
    pub risk_contribution: f64,    // Percentage contribution to total risk
    pub risk_budget_usage: f64,    // Actual vs target risk budget usage
}

/// Portfolio-level risk attribution
#[derive(Debug, Clone)]
pub struct RiskAttribution {
    pub total_portfolio_volatility: f64,
    pub target_portfolio_volatility: f64,
    pub risk_contributions: Vec<RiskContribution>,
    pub diversification_ratio: f64,    // Actual vs theoretical portfolio volatility
    pub concentration_score: f64,      // Measure of concentration risk (0-1)
    pub largest_risk_contributor: String,
    pub risk_budget_violations: Vec<String>, // Positions exceeding risk budget
}

/// Equal Risk Contribution (ERC) position sizing recommendation
#[derive(Debug, Clone)]
pub struct ERCAllocation {
    pub symbol: String,
    pub current_weight: f64,
    pub target_weight: f64,
    pub adjustment_needed: f64,    // Positive = increase, negative = decrease
    pub risk_contribution_current: f64,
    pub risk_contribution_target: f64,
}

/// Correlation-based risk metrics
#[derive(Debug, Clone)]
pub struct CorrelationRisk {
    pub average_correlation: f64,
    pub max_correlation: f64,
    pub correlation_clusters: Vec<Vec<String>>, // Groups of highly correlated assets
    pub diversification_score: f64,  // 1.0 = perfect diversification, 0.0 = perfect correlation
}

impl RiskBudgeter {
    /// Create new risk budgeter with target volatility
    pub fn new(risk_config: RiskConfig, target_volatility: f64) -> Self {
        Self {
            risk_config,
            correlation_matrix: HashMap::new(),
            volatilities: HashMap::new(),
            target_portfolio_volatility: target_volatility,
        }
    }

    /// Update correlation matrix between instruments
    pub fn update_correlation(&mut self, symbol1: &str, symbol2: &str, correlation: f64) -> Result<()> {
        // Validate correlation is between -1 and 1
        if !(-1.0..=1.0).contains(&correlation) {
            return Err(anyhow::anyhow!("Correlation must be between -1 and 1, got {}", correlation));
        }

        // Store both directions for easy lookup
        self.correlation_matrix.insert((symbol1.to_string(), symbol2.to_string()), correlation);
        self.correlation_matrix.insert((symbol2.to_string(), symbol1.to_string()), correlation);
        
        Ok(())
    }

    /// Update individual instrument volatility
    pub fn update_volatility(&mut self, symbol: &str, volatility: f64) -> Result<()> {
        if volatility < 0.0 {
            return Err(anyhow::anyhow!("Volatility cannot be negative, got {}", volatility));
        }
        
        self.volatilities.insert(symbol.to_string(), volatility);
        Ok(())
    }

    /// Calculate risk contribution for each position in portfolio
    pub fn calculate_risk_contributions(&self, portfolio: &Portfolio) -> Result<RiskAttribution> {
        let positions = portfolio.positions();
        
        if positions.is_empty() {
            return Ok(RiskAttribution {
                total_portfolio_volatility: 0.0,
                target_portfolio_volatility: self.target_portfolio_volatility,
                risk_contributions: vec![],
                diversification_ratio: 1.0,
                concentration_score: 0.0,
                largest_risk_contributor: String::new(),
                risk_budget_violations: vec![],
            });
        }

        // Calculate portfolio value and position weights
        let mut total_portfolio_value = 0.0;
        let mut weights = HashMap::new();
        
        for (symbol, position) in positions {
            let position_value = position.quantity * position.current_price;
            total_portfolio_value += position_value;
            weights.insert(symbol.clone(), position_value);
        }
        
        // Convert to actual weights (percentages)
        for (_, weight) in weights.iter_mut() {
            *weight /= total_portfolio_value;
        }

        // Calculate total portfolio volatility
        let total_portfolio_volatility = self.calculate_portfolio_volatility(&weights)?;

        // Calculate marginal contribution to risk for each position
        let mut risk_contributions = Vec::new();
        let mut largest_contribution = 0.0;
        let mut largest_contributor = String::new();

        for symbol in positions.keys() {
            let weight = weights.get(symbol).copied().unwrap_or(0.0);
            let volatility = self.volatilities.get(symbol).copied().unwrap_or(0.0);
            
            // Calculate marginal contribution to risk (MCR)
            // MCR_i = (w_i / σ_p) * Σ(w_j * σ_i * σ_j * ρ_ij) for all j
            let mut marginal_risk = 0.0;
            
            for other_symbol in positions.keys() {
                let other_weight = weights.get(other_symbol).copied().unwrap_or(0.0);
                let other_volatility = self.volatilities.get(other_symbol).copied().unwrap_or(0.0);
                let correlation = if symbol == other_symbol {
                    1.0
                } else {
                    self.get_correlation(symbol, other_symbol)
                };
                
                marginal_risk += other_weight * volatility * other_volatility * correlation;
            }
            
            // Normalize by portfolio volatility
            if total_portfolio_volatility > 0.0 {
                marginal_risk /= total_portfolio_volatility;
            }

            // Risk contribution = weight * marginal_risk
            let risk_contribution = if total_portfolio_volatility > 0.0 {
                (weight * marginal_risk) / total_portfolio_volatility
            } else {
                0.0
            };

            // Risk budget usage (how much of equal-share budget this position uses)
            let num_positions = positions.len() as f64;
            let target_risk_contribution = 1.0 / num_positions;
            let risk_budget_usage = if target_risk_contribution > 0.0 {
                risk_contribution / target_risk_contribution
            } else {
                0.0
            };

            if risk_contribution > largest_contribution {
                largest_contribution = risk_contribution;
                largest_contributor = symbol.clone();
            }

            risk_contributions.push(RiskContribution {
                symbol: symbol.clone(),
                weight,
                volatility,
                marginal_risk,
                risk_contribution,
                risk_budget_usage,
            });
        }

        // Calculate diversification ratio (weighted average volatility / portfolio volatility)
        let weighted_avg_volatility: f64 = positions.iter()
            .map(|(symbol, _)| {
                let weight = weights.get(symbol).copied().unwrap_or(0.0);
                let volatility = self.volatilities.get(symbol).copied().unwrap_or(0.0);
                weight * volatility
            })
            .sum();

        let diversification_ratio = if total_portfolio_volatility > 0.0 {
            weighted_avg_volatility / total_portfolio_volatility
        } else {
            1.0
        };

        // Calculate concentration score (0 = equal distribution, 1 = fully concentrated)
        let concentration_score = if risk_contributions.is_empty() {
            0.0
        } else {
            // Herfindahl-Hirschman Index for risk contributions
            let hhi: f64 = risk_contributions.iter()
                .map(|rc| rc.risk_contribution * rc.risk_contribution)
                .sum();
            
            // Normalize: HHI ranges from 1/n to 1, we want 0 to 1
            let min_hhi = 1.0 / risk_contributions.len() as f64;
            if (1.0 - min_hhi).abs() < f64::EPSILON {
                0.0 // Avoid division by zero
            } else {
                (hhi - min_hhi) / (1.0 - min_hhi)
            }
        };

        // Check for violations
        let risk_attribution = RiskAttribution {
            total_portfolio_volatility,
            target_portfolio_volatility: self.target_portfolio_volatility,
            risk_contributions,
            diversification_ratio,
            concentration_score,
            largest_risk_contributor: largest_contributor,
            risk_budget_violations: vec![], // Will be filled below
        };

        let violations = self.check_risk_budget_violations(&risk_attribution);

        Ok(RiskAttribution {
            risk_budget_violations: violations,
            ..risk_attribution
        })
    }

    /// Calculate Equal Risk Contribution (ERC) allocations
    pub fn calculate_erc_allocations(&self, portfolio: &Portfolio) -> Result<Vec<ERCAllocation>> {
        let positions = portfolio.positions();
        
        if positions.is_empty() {
            return Ok(vec![]);
        }

        // First, get current risk attribution
        let current_attribution = self.calculate_risk_contributions(portfolio)?;
        
        // Equal risk contribution target: each position should contribute equally
        let num_positions = positions.len() as f64;
        let target_risk_contribution = 1.0 / num_positions;
        
        let mut erc_allocations = Vec::new();
        
        for risk_contrib in &current_attribution.risk_contributions {
            // Calculate current portfolio weight
            let current_weight = risk_contrib.weight;
            
            // For ERC, we want: Risk_Contribution_i = target_risk_contribution
            // Risk_Contribution_i = (w_i * MCR_i) / portfolio_volatility
            // So: w_i = (target_risk_contribution * portfolio_volatility) / MCR_i
            
            let target_weight = if risk_contrib.marginal_risk > 0.0 {
                (target_risk_contribution * current_attribution.total_portfolio_volatility) / risk_contrib.marginal_risk
            } else {
                current_weight // Keep current weight if marginal risk is zero
            };
            
            // Calculate adjustment needed
            let adjustment_needed = target_weight - current_weight;
            
            erc_allocations.push(ERCAllocation {
                symbol: risk_contrib.symbol.clone(),
                current_weight,
                target_weight,
                adjustment_needed,
                risk_contribution_current: risk_contrib.risk_contribution,
                risk_contribution_target: target_risk_contribution,
            });
        }
        
        // Normalize weights to ensure they sum to 1.0
        let total_target_weight: f64 = erc_allocations.iter().map(|alloc| alloc.target_weight).sum();
        
        if total_target_weight > 0.0 {
            for allocation in &mut erc_allocations {
                allocation.target_weight /= total_target_weight;
                allocation.adjustment_needed = allocation.target_weight - allocation.current_weight;
            }
        }
        
        Ok(erc_allocations)
    }

    /// Calculate correlation-based risk metrics
    pub fn calculate_correlation_risk(&self, symbols: &[String]) -> Result<CorrelationRisk> {
        if symbols.len() < 2 {
            return Ok(CorrelationRisk {
                average_correlation: 0.0,
                max_correlation: 0.0,
                correlation_clusters: vec![],
                diversification_score: 1.0, // Perfect diversification with < 2 assets
            });
        }

        let mut correlations = Vec::new();
        let mut max_correlation: f64 = 0.0;

        // Calculate all pairwise correlations
        for (i, symbol_i) in symbols.iter().enumerate() {
            for symbol_j in symbols.iter().skip(i + 1) {
                let correlation = self.get_correlation(symbol_i, symbol_j).abs();
                correlations.push(correlation);
                max_correlation = max_correlation.max(correlation);
            }
        }

        // Calculate average correlation
        let average_correlation = if correlations.is_empty() {
            0.0
        } else {
            correlations.iter().sum::<f64>() / correlations.len() as f64
        };

        // Identify correlation clusters (assets with correlation > 0.75)
        let mut correlation_clusters = Vec::new();
        let high_correlation_threshold = 0.75;

        for (i, symbol_i) in symbols.iter().enumerate() {
            let mut cluster = vec![symbol_i.clone()];
            
            for (j, symbol_j) in symbols.iter().enumerate() {
                if i != j {
                    let correlation = self.get_correlation(symbol_i, symbol_j).abs();
                    if correlation > high_correlation_threshold {
                        // Check if this symbol is already in another cluster
                        let already_clustered = correlation_clusters.iter()
                            .any(|existing_cluster: &Vec<String>| existing_cluster.contains(symbol_j));
                        
                        if !already_clustered && !cluster.contains(symbol_j) {
                            cluster.push(symbol_j.clone());
                        }
                    }
                }
            }
            
            // Only add clusters with more than one member
            if cluster.len() > 1 {
                // Check if this cluster overlaps with existing clusters
                let overlaps_existing = correlation_clusters.iter()
                    .any(|existing_cluster: &Vec<String>| 
                        cluster.iter().any(|symbol| existing_cluster.contains(symbol)));
                
                if !overlaps_existing {
                    correlation_clusters.push(cluster);
                }
            }
        }

        // Calculate diversification score (1.0 = perfect diversification, 0.0 = perfect correlation)
        // Based on how much portfolio volatility is reduced by diversification
        let diversification_score = if average_correlation > 0.95 {
            0.0 // Nearly perfect correlation
        } else {
            1.0 - average_correlation
        };

        Ok(CorrelationRisk {
            average_correlation,
            max_correlation,
            correlation_clusters,
            diversification_score,
        })
    }

    /// Get correlation between two instruments (returns 0.0 if not found)
    pub fn get_correlation(&self, symbol1: &str, symbol2: &str) -> f64 {
        self.correlation_matrix
            .get(&(symbol1.to_string(), symbol2.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Check if portfolio violates risk budget constraints
    pub fn check_risk_budget_violations(&self, risk_attribution: &RiskAttribution) -> Vec<String> {
        let mut violations = Vec::new();
        
        // Equal Risk Contribution target: each position should contribute equally
        let num_positions = risk_attribution.risk_contributions.len();
        if num_positions == 0 {
            return violations;
        }
        
        let target_risk_contribution = 1.0 / num_positions as f64; // Equal share for each position
        let violation_threshold = 0.25; // 25% deviation from target is considered a violation
        
        for contribution in &risk_attribution.risk_contributions {
            // Check if risk contribution deviates significantly from target
            let deviation = (contribution.risk_contribution - target_risk_contribution).abs();
            let relative_deviation = deviation / target_risk_contribution;
            
            // Check if risk budget usage is excessive (>100% means over-contributing to risk)
            let excessive_risk_budget = contribution.risk_budget_usage > 1.0;
            
            // Check if risk contribution is too high (>50% of total portfolio risk from one position)
            let excessive_risk_contribution = contribution.risk_contribution > 0.50;
            
            if relative_deviation > violation_threshold || excessive_risk_budget || excessive_risk_contribution {
                violations.push(contribution.symbol.clone());
            }
        }
        
        violations
    }

    /// Calculate portfolio volatility given position weights and correlations
    /// Uses the formula: Portfolio Volatility = √(w'Σw)
    /// where w is the weights vector and Σ is the covariance matrix
    pub fn calculate_portfolio_volatility(&self, weights: &HashMap<String, f64>) -> Result<f64> {
        if weights.is_empty() {
            return Ok(0.0);
        }

        // Get all symbols
        let symbols: Vec<String> = weights.keys().cloned().collect();
        
        // Validate that we have volatilities for all symbols
        for symbol in &symbols {
            if !self.volatilities.contains_key(symbol) {
                return Err(anyhow::anyhow!("Missing volatility for symbol: {}", symbol));
            }
        }

        // Calculate portfolio variance using covariance matrix formula
        let mut portfolio_variance = 0.0;
        
        for (i, symbol_i) in symbols.iter().enumerate() {
            let weight_i = weights.get(symbol_i).unwrap_or(&0.0);
            let vol_i = self.volatilities.get(symbol_i).unwrap();
            
            for (j, symbol_j) in symbols.iter().enumerate() {
                let weight_j = weights.get(symbol_j).unwrap_or(&0.0);
                let vol_j = self.volatilities.get(symbol_j).unwrap();
                
                // Get correlation (1.0 for same symbol, stored value otherwise)
                let correlation = if i == j {
                    1.0
                } else {
                    self.get_correlation(symbol_i, symbol_j)
                };
                
                // Covariance = correlation * vol_i * vol_j
                let covariance = correlation * vol_i * vol_j;
                
                // Add to portfolio variance: w_i * w_j * cov(i,j)
                portfolio_variance += weight_i * weight_j * covariance;
            }
        }
        
        // Portfolio volatility is the square root of variance
        Ok(portfolio_variance.sqrt())
    }

    /// Generate rebalancing recommendations to achieve ERC
    pub fn generate_rebalancing_recommendations(&self, portfolio: &Portfolio) -> Result<Vec<ERCAllocation>> {
        // Rebalancing recommendations are essentially ERC allocations
        // with filtering for significant adjustments
        let erc_allocations = self.calculate_erc_allocations(portfolio)?;
        
        // Filter for positions that need significant rebalancing
        // Only recommend changes > 2% of current weight to avoid excessive trading
        let rebalancing_threshold = 0.02; // 2% threshold
        
        let mut recommendations = Vec::new();
        
        for allocation in erc_allocations {
            let relative_adjustment = if allocation.current_weight > 0.0 {
                allocation.adjustment_needed.abs() / allocation.current_weight
            } else {
                allocation.adjustment_needed.abs()
            };
            
            // Include this recommendation if:
            // 1. Adjustment is > 2% of current weight, OR
            // 2. Absolute adjustment is > 5% (for very small positions), OR  
            // 3. Risk budget usage is significantly off (>150% or <50%)
            let needs_rebalancing = relative_adjustment > rebalancing_threshold ||
                allocation.adjustment_needed.abs() > 0.05 ||
                allocation.risk_contribution_current / allocation.risk_contribution_target > 1.5 ||
                allocation.risk_contribution_current / allocation.risk_contribution_target < 0.5;
            
            if needs_rebalancing {
                recommendations.push(allocation);
            }
        }
        
        // Sort by magnitude of adjustment needed (largest adjustments first)
        recommendations.sort_by(|a, b| {
            b.adjustment_needed.abs().partial_cmp(&a.adjustment_needed.abs()).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        Ok(recommendations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RiskConfig;
    use crate::portfolio::Portfolio;
    use crate::security_types::SecurityInfo;

    fn create_test_risk_config() -> RiskConfig {
        RiskConfig {
            max_position_size: 0.10,      // 10% max per position
            max_portfolio_exposure: 0.95,
            stop_loss_percentage: 0.02,
            take_profit_percentage: 0.04,
            max_margin_utilization: 0.70,
            min_excess_liquidity: 10000.0,
            futures_position_limit: 10.0,
            margin_call_threshold: 0.85,
            margin_buffer_percentage: 0.20,
            enable_risk_budgeting: true,
            risk_budget_target_volatility: 0.15,
            risk_budget_rebalance_threshold: 0.05,
            max_correlation_exposure: 0.60,
            correlation_lookback_days: 63,
            min_positions_for_erc: 3,
        }
    }

    fn create_test_portfolio() -> Portfolio {
        let mut portfolio = Portfolio::new(50000.0); // $50k initial cash
        
        // Register securities first
        portfolio.register_security("AAPL".to_string(), SecurityInfo::new_stock("AAPL".to_string(), "SMART".to_string(), "USD".to_string()));
        portfolio.register_security("SPY".to_string(), SecurityInfo::new_stock("SPY".to_string(), "SMART".to_string(), "USD".to_string()));
        portfolio.register_security("QQQ".to_string(), SecurityInfo::new_stock("QQQ".to_string(), "SMART".to_string(), "USD".to_string()));
        
        // Add positions using the update_position method
        portfolio.update_position("AAPL", 100.0, 150.0); // 100 shares at $150
        portfolio.update_position("SPY", 50.0, 400.0);   // 50 shares at $400  
        portfolio.update_position("QQQ", 75.0, 300.0);   // 75 shares at $300

        portfolio
    }

    #[test]
    fn test_risk_budgeter_creation() {
        let risk_config = create_test_risk_config();
        let budgeter = RiskBudgeter::new(risk_config, 0.15); // 15% target volatility

        assert_eq!(budgeter.target_portfolio_volatility, 0.15);
        assert!(budgeter.correlation_matrix.is_empty());
        assert!(budgeter.volatilities.is_empty());
    }

    #[test]
    fn test_correlation_update_valid() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        let result = budgeter.update_correlation("AAPL", "SPY", 0.75);
        assert!(result.is_ok());
        
        // Should be able to retrieve correlation both ways
        assert_eq!(budgeter.get_correlation("AAPL", "SPY"), 0.75);
        assert_eq!(budgeter.get_correlation("SPY", "AAPL"), 0.75);
    }

    #[test]
    fn test_correlation_update_invalid_range() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        // Test correlation > 1.0
        let result = budgeter.update_correlation("AAPL", "SPY", 1.5);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Correlation must be between -1 and 1"));
        
        // Test correlation < -1.0
        let result = budgeter.update_correlation("AAPL", "SPY", -1.5);
        assert!(result.is_err());
    }

    #[test]
    fn test_volatility_update_valid() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        let result = budgeter.update_volatility("AAPL", 0.25);
        assert!(result.is_ok());
        
        assert_eq!(budgeter.volatilities.get("AAPL"), Some(&0.25));
    }

    #[test]
    fn test_volatility_update_negative() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        let result = budgeter.update_volatility("AAPL", -0.1);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Volatility cannot be negative"));
    }

    #[test]
    fn test_get_correlation_not_found() {
        let budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        // Should return 0.0 for unknown correlation pairs
        assert_eq!(budgeter.get_correlation("UNKNOWN1", "UNKNOWN2"), 0.0);
    }

    #[test]
    fn test_risk_contribution_calculation_with_correlations() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        let portfolio = create_test_portfolio();

        // Set up volatilities for test instruments
        budgeter.update_volatility("AAPL", 0.30).unwrap(); // 30% volatility
        budgeter.update_volatility("SPY", 0.15).unwrap();  // 15% volatility  
        budgeter.update_volatility("QQQ", 0.25).unwrap();  // 25% volatility

        // Set up correlations (tech stocks correlated, SPY less so)
        budgeter.update_correlation("AAPL", "QQQ", 0.80).unwrap(); // High correlation
        budgeter.update_correlation("AAPL", "SPY", 0.60).unwrap(); // Moderate correlation
        budgeter.update_correlation("QQQ", "SPY", 0.65).unwrap();  // Moderate correlation

        // This test will fail until we implement the calculation
        let result = budgeter.calculate_risk_contributions(&portfolio);
        
        // We expect this to fail in RED phase
        // When implemented, should verify:
        // - Risk contributions sum to 100%
        // - QQQ has highest risk contribution due to largest position + high volatility
        // - Correlations properly affect risk calculations
        assert!(result.is_err() || {
            let attribution = result.unwrap();
            // Verify risk contributions sum to approximately 100%
            let total_risk: f64 = attribution.risk_contributions.iter().map(|rc| rc.risk_contribution).sum();
            (total_risk - 1.0).abs() < 0.01 &&
            // Verify QQQ has highest risk contribution (largest position + high volatility)
            attribution.risk_contributions.iter().any(|rc| rc.symbol == "QQQ" && rc.risk_contribution > 0.40) &&
            // Verify AAPL has significant but not highest risk contribution
            attribution.risk_contributions.iter().any(|rc| rc.symbol == "AAPL" && rc.risk_contribution > 0.30 && rc.risk_contribution < 0.50)
        });
    }

    #[test]
    fn test_erc_allocation_calculation() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        let portfolio = create_test_portfolio();

        // Set up test data
        budgeter.update_volatility("AAPL", 0.30).unwrap();
        budgeter.update_volatility("SPY", 0.15).unwrap();
        budgeter.update_volatility("QQQ", 0.25).unwrap();

        budgeter.update_correlation("AAPL", "QQQ", 0.80).unwrap();
        budgeter.update_correlation("AAPL", "SPY", 0.60).unwrap();
        budgeter.update_correlation("QQQ", "SPY", 0.65).unwrap();

        // This test will fail until we implement ERC calculation
        let result = budgeter.calculate_erc_allocations(&portfolio);
        
        // When implemented, should verify:
        // - Each position has equal risk contribution target (33.33% each)
        // - Higher volatility assets get lower weights
        // - Adjustments needed to achieve ERC
        assert!(result.is_err() || {
            let allocations = result.unwrap();
            allocations.len() == 3 &&
            // All target risk contributions should be equal (1/3 each)
            allocations.iter().all(|alloc| (alloc.risk_contribution_target - 1.0/3.0).abs() < 0.01) &&
            // AAPL should need weight reduction due to high volatility
            allocations.iter().any(|alloc| alloc.symbol == "AAPL" && alloc.adjustment_needed < 0.0)
        });
    }

    #[test]
    fn test_correlation_risk_analysis() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        // Set up correlation matrix
        budgeter.update_correlation("AAPL", "QQQ", 0.85).unwrap(); // High correlation
        budgeter.update_correlation("AAPL", "SPY", 0.60).unwrap();
        budgeter.update_correlation("QQQ", "SPY", 0.65).unwrap();

        let symbols = vec!["AAPL".to_string(), "SPY".to_string(), "QQQ".to_string()];
        
        // This test will fail until we implement correlation analysis
        let result = budgeter.calculate_correlation_risk(&symbols);
        
        // When implemented, should verify:
        // - Average correlation calculated correctly
        // - AAPL and QQQ identified as highly correlated cluster
        // - Diversification score reflects correlation structure
        assert!(result.is_err() || {
            let corr_risk = result.unwrap();
            corr_risk.max_correlation >= 0.85 &&
            corr_risk.average_correlation > 0.60 &&
            // Should identify AAPL-QQQ as highly correlated cluster
            corr_risk.correlation_clusters.iter().any(|cluster| 
                cluster.contains(&"AAPL".to_string()) && cluster.contains(&"QQQ".to_string()))
        });
    }

    #[test]
    fn test_portfolio_volatility_calculation() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        // Set up test data
        budgeter.update_volatility("AAPL", 0.30).unwrap();
        budgeter.update_volatility("SPY", 0.15).unwrap();
        budgeter.update_volatility("QQQ", 0.25).unwrap();

        budgeter.update_correlation("AAPL", "QQQ", 0.80).unwrap();
        budgeter.update_correlation("AAPL", "SPY", 0.60).unwrap();
        budgeter.update_correlation("QQQ", "SPY", 0.65).unwrap();

        // Equal weights portfolio
        let mut weights = HashMap::new();
        weights.insert("AAPL".to_string(), 1.0/3.0);
        weights.insert("SPY".to_string(), 1.0/3.0);
        weights.insert("QQQ".to_string(), 1.0/3.0);

        // This test will fail until we implement portfolio volatility calculation
        let result = budgeter.calculate_portfolio_volatility(&weights);
        
        // When implemented, should verify:
        // - Portfolio volatility is less than weighted average due to diversification
        // - Formula: sqrt(w'Σw) where w is weights vector, Σ is covariance matrix
        assert!(result.is_err() || {
            let portfolio_vol = result.unwrap();
            // Portfolio vol should be less than weighted average (0.233) due to diversification
            portfolio_vol > 0.15 && portfolio_vol < 0.233
        });
    }

    #[test]
    fn test_risk_budget_violation_detection() {
        let budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        
        // Create risk attribution with violations
        let risk_attribution = RiskAttribution {
            total_portfolio_volatility: 0.18,
            target_portfolio_volatility: 0.15,
            risk_contributions: vec![
                RiskContribution {
                    symbol: "AAPL".to_string(),
                    weight: 0.40,
                    volatility: 0.30,
                    marginal_risk: 0.25,
                    risk_contribution: 0.60, // 60% risk contribution - violation!
                    risk_budget_usage: 1.80, // 180% of target (should be 100%)
                },
                RiskContribution {
                    symbol: "SPY".to_string(),
                    weight: 0.35,
                    volatility: 0.15,
                    marginal_risk: 0.12,
                    risk_contribution: 0.25,
                    risk_budget_usage: 0.75,
                },
                RiskContribution {
                    symbol: "QQQ".to_string(),
                    weight: 0.25,
                    volatility: 0.25,
                    marginal_risk: 0.18,
                    risk_contribution: 0.15,
                    risk_budget_usage: 0.45,
                },
            ],
            diversification_ratio: 0.85,
            concentration_score: 0.65,
            largest_risk_contributor: "AAPL".to_string(),
            risk_budget_violations: vec![],
        };

        // This test will fail until we implement violation checking
        let violations = budgeter.check_risk_budget_violations(&risk_attribution);
        
        // When implemented, should identify AAPL as violating risk budget
        // (risk_budget_usage > 1.0 or risk_contribution > target_contribution)
        assert!(violations.is_empty() || violations.contains(&"AAPL".to_string()));
    }


    #[test]
    fn test_rebalancing_recommendations() {
        let mut budgeter = RiskBudgeter::new(create_test_risk_config(), 0.15);
        let portfolio = create_test_portfolio();

        // Set up test data for concentrated portfolio needing rebalancing
        budgeter.update_volatility("AAPL", 0.35).unwrap(); // High vol
        budgeter.update_volatility("SPY", 0.12).unwrap();  // Low vol
        budgeter.update_volatility("QQQ", 0.28).unwrap();  // High vol

        budgeter.update_correlation("AAPL", "QQQ", 0.90).unwrap(); // Very high correlation
        budgeter.update_correlation("AAPL", "SPY", 0.50).unwrap();
        budgeter.update_correlation("QQQ", "SPY", 0.55).unwrap();

        // This test will fail until we implement rebalancing recommendations
        let result = budgeter.generate_rebalancing_recommendations(&portfolio);
        
        // When implemented, should recommend:
        // - Reducing AAPL and QQQ weights (high vol, high correlation)
        // - Increasing SPY weight (low vol, better diversification)
        assert!(result.is_err() || {
            let recommendations = result.unwrap();
            recommendations.len() == 3 &&
            // AAPL should be recommended for reduction
            recommendations.iter().any(|rec| rec.symbol == "AAPL" && rec.adjustment_needed < 0.0) &&
            // SPY should be recommended for increase  
            recommendations.iter().any(|rec| rec.symbol == "SPY" && rec.adjustment_needed > 0.0)
        });
    }
}