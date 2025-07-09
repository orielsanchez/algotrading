# Systematic Momentum Trading Bot

A production-ready Rust-based algorithmic trading system implementing Robert Carver's systematic trading framework with volatility targeting, multi-timeframe signals, and comprehensive risk management.

[![Rust](https://img.shields.io/badge/rust-1.87+-orange.svg)](https://www.rust-lang.org/)
[![Buy Me A Coffee](https://img.shields.io/badge/Buy%20Me%20A%20Coffee-support-yellow.svg)](https://www.buymeacoffee.com/orielsanchez)

## Overview

This systematic trading bot implements advanced momentum strategies with:
- **Volatility Targeting**: 25% annual portfolio volatility with EWMA calculation (32-day half-life)
- **Multi-Timeframe Momentum**: 4 timeframe signals (2-8, 4-16, 8-32, 16-64 days) with forecast diversification
- **Breakout Detection**: Volatility-adjusted breakout signals with multi-timeframe consensus
- **Smart Order Execution**: Limit orders with 1% price improvement over market prices
- **Risk Management**: Position inertia buffers, exposure limits, and dynamic position sizing

## Prerequisites

1. **Interactive Brokers Account** with TWS or IB Gateway
2. **Rust** (latest stable version)
3. **TWS/IB Gateway** running locally (default port 7497 for paper trading, 7496 for live)

## Setup

1. Clone the repository and navigate to the project directory
2. Configure your trading parameters in `config.json`
3. Ensure TWS or IB Gateway is running and API connections are enabled

## Configuration

Edit `config.json` to customize:

- **TWS Connection**: Host, port, and client ID
- **Strategy Parameters**: 
  - Symbols to trade (stocks and forex pairs)
  - Momentum timeframes and lookback periods
  - Volatility targeting parameters
  - Signal combination weights
  - Rebalance frequency
- **Order Execution**:
  - Order types (market vs limit orders)
  - Limit order price offsets
  - Position inertia thresholds
- **Risk Management**:
  - Portfolio volatility target (25% annual)
  - Position size limits
  - Portfolio exposure limits
  - Forecast diversification multipliers

## Running the Bot

```bash
# Set logging level (optional)
export RUST_LOG=info

# Run the bot
cargo run
```

The bot will:
1. Connect to TWS/IB Gateway
2. Subscribe to market data for configured symbols
3. Calculate multi-timeframe momentum and breakout signals
4. Apply volatility targeting for position sizing
5. Generate and execute trading signals with optimal order types
6. Monitor portfolio risk and rebalance positions
7. Log all activities and comprehensive portfolio analytics

## Architecture

- `main.rs` - Application entry point and trading loop
- `config.rs` - Configuration with volatility targeting
- `connection.rs` - IBKR API with enhanced order types
- `market_data.rs` - Multi-timeframe data processing
- `momentum.rs` - Multi-timeframe momentum signals
- `breakout.rs` - Breakout detection system
- `volatility.rs` - Carver's volatility targeting
- `order_types.rs` - Enhanced order management
- `portfolio.rs` - Position tracking and analytics
- `risk.rs` - Risk management and position sizing

## Safety Features

- Paper trading configuration by default (port 7497)
- Volatility-adjusted position sizing with dynamic risk control
- Portfolio exposure limits and position inertia buffers
- Signal strength validation and quality multipliers
- Comprehensive logging and risk monitoring
- Graceful shutdown on Ctrl+C

## Monitoring

The bot logs:
- Connection status and API health
- Market data updates and validation
- Multi-timeframe momentum and breakout calculations
- Signal generation with strength scores
- Volatility targeting and position sizing decisions
- Order executions with limit order performance
- Portfolio statistics and risk metrics

## Features

- **Carver Framework**: Implements systematic trading principles with volatility targeting
- **Multi-Asset Support**: Trades stocks and forex pairs with appropriate risk scaling
- **Advanced Signal Generation**: Combines momentum and breakout signals across multiple timeframes
- **Smart Execution**: Limit orders with price improvement for better fill prices
- **Risk Management**: Comprehensive position sizing and portfolio risk controls
- **Production Ready**: Robust error handling, logging, and connection management

## Notes

- Uses `ibapi` Rust crate for Interactive Brokers TWS communication
- Ensure TWS API settings allow connections from localhost
- Test thoroughly in paper trading before deploying with real capital
- Monitor logs for connection, execution, and risk management events
- Follows Robert Carver's "Systematic Trading" methodology