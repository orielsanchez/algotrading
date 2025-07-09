use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityType {
    Stock,
    Future,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub symbol: String,
    pub security_type: SecurityType,
    pub exchange: String,
    pub currency: String,
    pub contract_specs: Option<FuturesContract>,
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
        }
    }
}