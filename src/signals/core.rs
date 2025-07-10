//! Core signal generation traits and shared data structures
//!
//! Provides the unified interface for all signal types following Carver's systematic trading framework.

use crate::market_data::{MarketDataHandler, TimeFrame};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Common signal quality indicators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalQuality {
    High,     // Strong confidence signal
    Medium,   // Moderate confidence signal
    Low,      // Weak confidence signal
    Filtered, // Signal filtered out due to poor conditions
}

/// Signal type classification for combination weighting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalType {
    Momentum,      // Trend-following signals
    Breakout,      // Price breakout signals
    Carry,         // Interest rate differential signals
    MeanReversion, // Bollinger, RSI signals
}

/// Core signal data shared by all signal types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalCore {
    pub symbol: String,
    pub timeframe: TimeFrame,
    pub signal_strength: f64, // Carver -20 to +20 range
    pub signal_type: SignalType,
    pub percentile_rank: f64,     // Percentile rank vs historical values
    pub volatility_adjusted: f64, // Volatility-normalized signal strength
    pub quality: SignalQuality,   // Signal confidence level
}

impl SignalCore {
    /// Create a new signal core with Carver range validation
    pub fn new(
        symbol: String,
        timeframe: TimeFrame,
        signal_strength: f64,
        signal_type: SignalType,
        percentile_rank: f64,
        volatility_adjusted: f64,
        quality: SignalQuality,
    ) -> Self {
        Self {
            symbol,
            timeframe,
            signal_strength: signal_strength.clamp(-20.0, 20.0), // Enforce Carver range
            signal_type,
            percentile_rank: percentile_rank.clamp(0.0, 1.0), // Enforce percentile range
            volatility_adjusted,
            quality,
        }
    }

    /// Check if signal is actionable (non-filtered and meaningful strength)
    pub fn is_actionable(&self) -> bool {
        self.quality != SignalQuality::Filtered && self.signal_strength.abs() > 1.0
    }

    /// Get directional bias (-1, 0, +1)
    pub fn direction(&self) -> i8 {
        if self.signal_strength > 1.0 {
            1
        } else if self.signal_strength < -1.0 {
            -1
        } else {
            0
        }
    }
}

/// Multi-timeframe analysis core shared by all signal types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTimeframeCore<T> {
    pub timeframe_signals: HashMap<TimeFrame, T>,
    pub composite_signal: f64, // Combined signal strength across timeframes
    pub consensus_strength: f64, // Agreement level across timeframes
    pub consensus_boost: f64,  // Carver-style consensus multiplier
    pub strongest_timeframe: Option<TimeFrame>, // Timeframe with strongest signal
}

impl<T> MultiTimeframeCore<T>
where
    T: Clone,
{
    /// Create new multi-timeframe analysis
    pub fn new(
        timeframe_signals: HashMap<TimeFrame, T>,
        composite_signal: f64,
        consensus_strength: f64,
        consensus_boost: f64,
        strongest_timeframe: Option<TimeFrame>,
    ) -> Self {
        Self {
            timeframe_signals,
            composite_signal: composite_signal.clamp(-20.0, 20.0), // Enforce Carver range
            consensus_strength: consensus_strength.clamp(0.0, 1.0), // Enforce 0-1 range
            consensus_boost,
            strongest_timeframe,
        }
    }

    /// Get signal for specific timeframe
    pub fn get_signal(&self, timeframe: &TimeFrame) -> Option<&T> {
        self.timeframe_signals.get(timeframe)
    }

    /// Check if consensus is strong (>= 67% agreement)
    pub fn has_strong_consensus(&self) -> bool {
        self.consensus_strength >= 0.67
    }

    /// Get number of active timeframes
    pub fn active_timeframes(&self) -> usize {
        self.timeframe_signals.len()
    }
}

/// Combined signals from all signal generators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedSignals {
    pub momentum: Option<SignalCore>,
    pub breakout: Option<SignalCore>,
    pub carry: Option<SignalCore>,
    pub mean_reversion: Option<SignalCore>,
    pub composite_strength: f64, // Final combined signal strength
    pub dominant_signal: Option<SignalType>, // Strongest contributing signal type
    pub agreement_score: f64,    // Cross-signal agreement level
}

impl CombinedSignals {
    /// Create empty combined signals
    pub fn empty() -> Self {
        Self {
            momentum: None,
            breakout: None,
            carry: None,
            mean_reversion: None,
            composite_strength: 0.0,
            dominant_signal: None,
            agreement_score: 0.0,
        }
    }

    /// Check if any signals are actionable
    pub fn has_actionable_signals(&self) -> bool {
        [
            &self.momentum,
            &self.breakout,
            &self.carry,
            &self.mean_reversion,
        ]
        .iter()
        .filter_map(|opt| opt.as_ref())
        .any(|signal| signal.is_actionable())
    }

    /// Get strongest signal strength
    pub fn max_signal_strength(&self) -> f64 {
        [
            &self.momentum,
            &self.breakout,
            &self.carry,
            &self.mean_reversion,
        ]
        .iter()
        .filter_map(|opt| opt.as_ref())
        .map(|signal| signal.signal_strength.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
    }
}

/// Base trait for all signal generators following Carver's framework
pub trait SignalGenerator {
    /// Signal-specific type (e.g., MomentumSignal, BreakoutSignal)
    type Signal: Clone + Send + Sync;

    /// Multi-timeframe metrics type
    type Metrics: Clone + Send + Sync;

    /// Calculate signal for a specific timeframe
    fn calculate_signal(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Signal>>;

    /// Calculate multi-timeframe analysis
    fn calculate_multi_timeframe(
        &self,
        symbol: &str,
        market_data: &MarketDataHandler,
    ) -> Result<Option<Self::Metrics>>;

    /// Extract signal strength in Carver -20 to +20 range
    fn extract_signal_strength(&self, signal: &Self::Signal) -> f64;

    /// Convert signal to common SignalCore representation
    fn to_signal_core(&self, signal: &Self::Signal) -> SignalCore;

    /// Get signal type identifier
    fn signal_type(&self) -> SignalType;

    /// Get timeframes this generator supports
    fn supported_timeframes(&self) -> Vec<TimeFrame> {
        vec![
            TimeFrame::Days2_8,
            TimeFrame::Days4_16,
            TimeFrame::Days8_32,
            TimeFrame::Days16_64,
        ]
    }

    /// Validate signal meets quality standards
    fn validate_signal(&self, signal: &Self::Signal) -> SignalQuality {
        let strength = self.extract_signal_strength(signal);
        if strength.abs() > 10.0 {
            SignalQuality::High
        } else if strength.abs() > 5.0 {
            SignalQuality::Medium
        } else if strength.abs() > 1.0 {
            SignalQuality::Low
        } else {
            SignalQuality::Filtered
        }
    }
}

/// Configuration for signal combination and weighting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWeights {
    pub momentum: f64,
    pub breakout: f64,
    pub carry: f64,
    pub mean_reversion: f64,
}

impl Default for SignalWeights {
    fn default() -> Self {
        Self {
            momentum: 0.5,        // Primary trend signal
            breakout: 0.3,        // Secondary confirmation
            carry: 0.15,          // Fundamental factor
            mean_reversion: 0.05, // Counter-trend filter
        }
    }
}

impl SignalWeights {
    /// Validate weights sum to 1.0
    pub fn validate(&self) -> Result<()> {
        let total = self.momentum + self.breakout + self.carry + self.mean_reversion;
        if (total - 1.0).abs() > 0.001 {
            return Err(anyhow::anyhow!(
                "Signal weights must sum to 1.0, got: {:.3}",
                total
            ));
        }
        Ok(())
    }

    /// Normalize weights to sum to 1.0
    pub fn normalize(&mut self) {
        let total = self.momentum + self.breakout + self.carry + self.mean_reversion;
        if total > 0.0 {
            self.momentum /= total;
            self.breakout /= total;
            self.carry /= total;
            self.mean_reversion /= total;
        }
    }
}
