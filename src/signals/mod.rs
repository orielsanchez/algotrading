//! Signal Generation Module
//!
//! Unified signal generation framework implementing Robert Carver's systematic trading approach.
//! This module provides a common interface for all signal types with shared utilities and patterns.

pub mod bollinger;
pub mod breakout;
pub mod carry;
pub mod coordinator;
pub mod core;
pub mod example;
pub mod momentum;
pub mod utils;

// Re-export core types for easy access
pub use carry::CarrySignalGenerator;
pub use coordinator::{CoordinatorBuilder, CoordinatorConfig, SignalCoordinator};
pub use core::{SignalCore, SignalGenerator, SignalQuality, SignalType, SignalWeights};

// Note: Other signal generators and utilities are available but not re-exported to reduce warnings
// Use directly from their modules when needed:
// - bollinger::BollingerSignalGenerator
// - breakout::BreakoutSignalGenerator  
// - momentum::MomentumSignalGenerator
// - utils::SignalUtils
// - crate::market_data::TimeFrame
