use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityType {
    Stock,
    Future,
    Forex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub symbol: String,
    pub security_type: SecurityType,
    pub exchange: String,
    pub currency: String,
    pub contract_specs: Option<FuturesContract>,
    pub forex_pair: Option<ForexPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForexPair {
    pub base_currency: String,
    pub quote_currency: String,
    pub pair_symbol: String, // e.g., "EUR.USD"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesContract {
    pub underlying: String,
    pub expiry: String,
    pub multiplier: f64,
    pub tick_size: f64,
    pub contract_month: String,
}

impl SecurityInfo {
    pub fn new_stock(symbol: String, exchange: String, currency: String) -> Self {
        Self {
            symbol,
            security_type: SecurityType::Stock,
            exchange,
            currency,
            contract_specs: None,
            forex_pair: None,
        }
    }

    pub fn new_forex(symbol: String, exchange: String, currency: String) -> Self {
        // Parse forex pair if in new format (EUR.USD)
        let forex_pair = if symbol.contains('.') {
            let parts: Vec<&str> = symbol.split('.').collect();
            if parts.len() == 2 {
                Some(ForexPair {
                    base_currency: parts[0].to_string(),
                    quote_currency: parts[1].to_string(),
                    pair_symbol: symbol.to_string(),
                })
            } else {
                None
            }
        } else {
            // Old format: symbol=EUR, currency=USD -> EUR/USD pair
            Some(ForexPair {
                base_currency: symbol.to_string(),
                quote_currency: currency.to_string(),
                pair_symbol: format!("{}.{}", symbol, currency),
            })
        };

        Self {
            symbol,
            security_type: SecurityType::Forex,
            exchange,
            currency,
            contract_specs: None,
            forex_pair,
        }
    }

    pub fn new_future(
        symbol: String,
        exchange: String,
        currency: String,
        underlying: String,
        expiry: String,
        multiplier: f64,
        tick_size: f64,
        contract_month: String,
    ) -> Self {
        Self {
            symbol,
            security_type: SecurityType::Future,
            exchange,
            currency,
            contract_specs: Some(FuturesContract {
                underlying,
                expiry,
                multiplier,
                tick_size,
                contract_month,
            }),
            forex_pair: None,
        }
    }

    pub fn get_contract_value(&self, price: f64) -> f64 {
        match &self.security_type {
            SecurityType::Stock => price,
            SecurityType::Future => {
                if let Some(contract) = &self.contract_specs {
                    price * contract.multiplier
                } else {
                    price
                }
            }
            SecurityType::Forex => price,
        }
    }

    pub fn get_position_value(&self, price: f64, quantity: f64) -> f64 {
        match &self.security_type {
            SecurityType::Stock => price * quantity,
            SecurityType::Future => {
                if let Some(contract) = &self.contract_specs {
                    price * quantity * contract.multiplier
                } else {
                    price * quantity
                }
            }
            SecurityType::Forex => {
                // For forex pairs:
                // price = quote currency per base currency (e.g., 1.0850 USD per EUR)
                // quantity = base currency units (e.g., 10000 EUR)
                // value = quantity * price (e.g., 10000 EUR * 1.0850 = 10850 USD)
                price * quantity
            }
        }
    }

    pub fn get_forex_description(&self) -> Option<String> {
        if let Some(ref pair) = self.forex_pair {
            Some(format!(
                "Trading {} {} against {} {}",
                self.symbol,
                pair.base_currency,
                self.currency,
                pair.quote_currency
            ))
        } else {
            None
        }
    }
}
