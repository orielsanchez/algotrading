# Momentum Trading Bot for Interactive Brokers

A Rust-based momentum trading bot that interfaces with Interactive Brokers TWS API.

## Overview

This bot implements a momentum-based trading strategy that:
- Tracks price momentum across multiple stocks
- Ranks stocks by momentum scores
- Automatically rebalances positions based on momentum signals
- Manages risk through position sizing and portfolio exposure limits

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
  - Symbols to trade
  - Lookback period for momentum calculation
  - Momentum threshold
  - Position sizing
  - Rebalance frequency
- **Risk Management**:
  - Maximum position size
  - Maximum portfolio exposure
  - Stop loss and take profit percentages

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
3. Calculate momentum scores at specified intervals
4. Generate and execute trading signals
5. Log all activities and portfolio statistics

## Architecture

- `main.rs` - Main entry point and trading loop
- `config.rs` - Configuration management
- `connection.rs` - TWS API connection handling
- `market_data.rs` - Real-time market data processing
- `momentum.rs` - Momentum strategy implementation
- `orders.rs` - Order management system
- `portfolio.rs` - Portfolio tracking and analytics

## Safety Features

- Paper trading configuration by default (port 7497)
- Position size limits
- Portfolio exposure limits
- Comprehensive logging
- Graceful shutdown on Ctrl+C

## Monitoring

The bot logs:
- Connection status
- Market data updates
- Momentum calculations
- Trading signals generated
- Order executions
- Portfolio statistics

## Notes

- This bot uses the `ibapi` Rust crate for TWS communication
- Ensure your TWS API settings allow connections from localhost
- Test thoroughly in paper trading before using with real money
- Monitor logs for any connection or execution issues