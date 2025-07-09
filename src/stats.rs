use anyhow::{Result, anyhow};
use statrs::statistics::Statistics;

/// Calculate the Sharpe ratio for a series of returns
///
/// # Arguments
/// * `returns` - Array of periodic returns (e.g., daily returns)
/// * `risk_free_rate` - Annual risk-free rate
/// * `periods_per_year` - Number of periods in a year (252 for daily, 12 for monthly)
pub fn sharpe_ratio(returns: &[f64], risk_free_rate: f64, periods_per_year: f64) -> Result<f64> {
    if returns.is_empty() {
        return Err(anyhow!("Cannot calculate Sharpe ratio for empty returns"));
    }

    let mean_return = returns.mean();
    let std_dev = returns.std_dev();

    if std_dev == 0.0 {
        return Err(anyhow!(
            "Cannot calculate Sharpe ratio: zero standard deviation"
        ));
    }

    // Convert risk-free rate to period rate
    let period_rf_rate = risk_free_rate / periods_per_year;

    // Calculate Sharpe ratio
    let sharpe = (mean_return - period_rf_rate) / std_dev * periods_per_year.sqrt();

    Ok(sharpe)
}

/// Calculate the Sortino ratio (downside deviation)
pub fn sortino_ratio(returns: &[f64], risk_free_rate: f64, periods_per_year: f64) -> Result<f64> {
    if returns.is_empty() {
        return Err(anyhow!("Cannot calculate Sortino ratio for empty returns"));
    }

    let mean_return = returns.mean();
    let period_rf_rate = risk_free_rate / periods_per_year;

    // Calculate downside deviation
    let negative_returns: Vec<f64> = returns
        .iter()
        .filter(|&&r| r < period_rf_rate)
        .map(|&r| (r - period_rf_rate).powi(2))
        .collect();

    if negative_returns.is_empty() {
        return Ok(f64::INFINITY); // No downside risk
    }

    let downside_dev = (negative_returns.iter().sum::<f64>() / returns.len() as f64).sqrt();

    if downside_dev == 0.0 {
        return Ok(f64::INFINITY);
    }

    let sortino = (mean_return - period_rf_rate) / downside_dev * periods_per_year.sqrt();
    Ok(sortino)
}

/// Calculate maximum drawdown from a series of cumulative returns or prices
pub fn max_drawdown(values: &[f64]) -> Result<f64> {
    if values.is_empty() {
        return Err(anyhow!("Cannot calculate max drawdown for empty series"));
    }

    let mut max_value = values[0];
    let mut max_dd = 0.0;

    for &value in values.iter().skip(1) {
        if value > max_value {
            max_value = value;
        }
        let drawdown = (value - max_value) / max_value;
        if drawdown < max_dd {
            max_dd = drawdown;
        }
    }

    Ok(max_dd.abs())
}

/// Calculate win rate from a series of returns
pub fn win_rate(returns: &[f64]) -> Result<f64> {
    if returns.is_empty() {
        return Err(anyhow!("Cannot calculate win rate for empty returns"));
    }

    let wins = returns.iter().filter(|&&r| r > 0.0).count();
    Ok(wins as f64 / returns.len() as f64)
}

/// Calculate profit factor (gross profits / gross losses)
pub fn profit_factor(returns: &[f64]) -> Result<f64> {
    let gross_profits: f64 = returns.iter().filter(|&&r| r > 0.0).sum();
    let gross_losses: f64 = returns.iter().filter(|&&r| r < 0.0).map(|r| r.abs()).sum();

    if gross_losses == 0.0 {
        return Ok(f64::INFINITY);
    }

    Ok(gross_profits / gross_losses)
}

/// Perform a t-test to check if mean return is significantly different from zero
pub fn t_test_returns(returns: &[f64], confidence_level: f64) -> Result<TTestResult> {
    use statrs::distribution::{ContinuousCDF, StudentsT};

    if returns.len() < 2 {
        return Err(anyhow!("Need at least 2 returns for t-test"));
    }

    let n = returns.len() as f64;
    let mean = returns.mean();
    let std_dev = returns.std_dev();
    let std_error = std_dev / n.sqrt();

    if std_error == 0.0 {
        return Err(anyhow!("Cannot perform t-test: zero standard error"));
    }

    let t_statistic = mean / std_error;
    let df = n - 1.0;

    let t_dist = StudentsT::new(0.0, 1.0, df)?;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_statistic.abs()));

    let critical_value = t_dist.inverse_cdf(1.0 - (1.0 - confidence_level) / 2.0);
    let is_significant = t_statistic.abs() > critical_value;

    Ok(TTestResult {
        t_statistic,
        p_value,
        is_significant,
        mean,
        std_error,
        confidence_level,
    })
}

#[derive(Debug, Clone)]
pub struct TTestResult {
    pub t_statistic: f64,
    pub p_value: f64,
    pub is_significant: bool,
    pub mean: f64,
    pub std_error: f64,
    pub confidence_level: f64,
}

/// Calculate information ratio (active return / tracking error)
pub fn information_ratio(returns: &[f64], benchmark_returns: &[f64]) -> Result<f64> {
    if returns.len() != benchmark_returns.len() {
        return Err(anyhow!("Returns and benchmark must have same length"));
    }

    if returns.is_empty() {
        return Err(anyhow!(
            "Cannot calculate information ratio for empty returns"
        ));
    }

    // Calculate active returns
    let active_returns: Vec<f64> = returns
        .iter()
        .zip(benchmark_returns.iter())
        .map(|(r, b)| r - b)
        .collect();

    let mean_active_return = (&active_returns).mean();
    let tracking_error = (&active_returns).std_dev();

    if tracking_error == 0.0 {
        return Ok(f64::INFINITY);
    }

    Ok(mean_active_return / tracking_error)
}

/// Calculate rolling correlation between two series
pub fn rolling_correlation(series1: &[f64], series2: &[f64], window: usize) -> Result<Vec<f64>> {
    if series1.len() != series2.len() {
        return Err(anyhow!("Series must have same length"));
    }

    if window > series1.len() {
        return Err(anyhow!("Window size larger than series length"));
    }

    let mut correlations = Vec::new();

    for i in 0..=(series1.len() - window) {
        let slice1 = &series1[i..i + window];
        let slice2 = &series2[i..i + window];

        let mean1 = slice1.mean();
        let mean2 = slice2.mean();

        let mut cov = 0.0;
        let mut var1 = 0.0;
        let mut var2 = 0.0;

        for j in 0..window {
            let diff1 = slice1[j] - mean1;
            let diff2 = slice2[j] - mean2;
            cov += diff1 * diff2;
            var1 += diff1 * diff1;
            var2 += diff2 * diff2;
        }

        let correlation = if var1 > 0.0 && var2 > 0.0 {
            cov / (var1.sqrt() * var2.sqrt())
        } else {
            0.0
        };

        correlations.push(correlation);
    }

    Ok(correlations)
}

/// Portfolio statistics calculator
pub struct PortfolioStats {
    pub total_return: f64,
    pub annualized_return: f64,
    pub volatility: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub trades_count: usize,
}

impl PortfolioStats {
    pub fn calculate(returns: &[f64], risk_free_rate: f64, periods_per_year: f64) -> Result<Self> {
        if returns.is_empty() {
            return Err(anyhow!(
                "Cannot calculate portfolio stats for empty returns"
            ));
        }

        // Calculate cumulative returns
        let mut cumulative = vec![1.0];
        for &r in returns {
            cumulative.push(cumulative.last().unwrap() * (1.0 + r));
        }

        let total_return = cumulative.last().unwrap() - 1.0;
        let periods = returns.len() as f64;
        let annualized_return = (1.0 + total_return).powf(periods_per_year / periods) - 1.0;

        let volatility = returns.std_dev() * periods_per_year.sqrt();
        let sharpe = sharpe_ratio(returns, risk_free_rate, periods_per_year)?;
        let sortino = sortino_ratio(returns, risk_free_rate, periods_per_year)?;
        let max_dd = max_drawdown(&cumulative)?;
        let win_rate = win_rate(returns)?;
        let profit_factor = profit_factor(returns)?;

        Ok(PortfolioStats {
            total_return,
            annualized_return,
            volatility,
            sharpe_ratio: sharpe,
            sortino_ratio: sortino,
            max_drawdown: max_dd,
            win_rate,
            profit_factor,
            trades_count: returns.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpe_ratio() {
        let returns = vec![0.01, 0.02, -0.01, 0.015, 0.005];
        let sharpe = sharpe_ratio(&returns, 0.02, 252.0).unwrap();
        assert!(sharpe > 0.0);
    }

    #[test]
    fn test_max_drawdown() {
        let values = vec![100.0, 110.0, 95.0, 105.0, 90.0, 100.0];
        let dd = max_drawdown(&values).unwrap();
        assert!((dd - 0.1818).abs() < 0.001); // ~18.18% drawdown
    }

    #[test]
    fn test_win_rate() {
        let returns = vec![0.01, -0.02, 0.03, -0.01, 0.02];
        let wr = win_rate(&returns).unwrap();
        assert_eq!(wr, 0.6); // 3 wins out of 5
    }

    #[test]
    fn test_t_test() {
        let returns = vec![0.01, 0.02, 0.015, 0.025, 0.03];
        let result = t_test_returns(&returns, 0.95).unwrap();
        assert!(result.is_significant);
        assert!(result.p_value < 0.05);
    }
}
