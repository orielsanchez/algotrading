use chrono::{Datelike, Local, NaiveDate, Weekday};
use anyhow::Result;

/// Futures contract months and their codes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContractMonth {
    January,   // F
    February,  // G
    March,     // H
    April,     // J
    May,       // K
    June,      // M
    July,      // N
    August,    // Q
    September, // U
    October,   // V
    November,  // X
    December,  // Z
}

impl ContractMonth {
    /// Get the month code for CME contracts
    pub fn code(&self) -> char {
        match self {
            ContractMonth::January => 'F',
            ContractMonth::February => 'G',
            ContractMonth::March => 'H',
            ContractMonth::April => 'J',
            ContractMonth::May => 'K',
            ContractMonth::June => 'M',
            ContractMonth::July => 'N',
            ContractMonth::August => 'Q',
            ContractMonth::September => 'U',
            ContractMonth::October => 'V',
            ContractMonth::November => 'X',
            ContractMonth::December => 'Z',
        }
    }

    /// Convert from month number (1-12)
    pub fn from_month(month: u32) -> Result<Self> {
        match month {
            1 => Ok(ContractMonth::January),
            2 => Ok(ContractMonth::February),
            3 => Ok(ContractMonth::March),
            4 => Ok(ContractMonth::April),
            5 => Ok(ContractMonth::May),
            6 => Ok(ContractMonth::June),
            7 => Ok(ContractMonth::July),
            8 => Ok(ContractMonth::August),
            9 => Ok(ContractMonth::September),
            10 => Ok(ContractMonth::October),
            11 => Ok(ContractMonth::November),
            12 => Ok(ContractMonth::December),
            _ => Err(anyhow::anyhow!("Invalid month number: {}", month)),
        }
    }

    /// Get the month number (1-12)
    pub fn to_month(&self) -> u32 {
        match self {
            ContractMonth::January => 1,
            ContractMonth::February => 2,
            ContractMonth::March => 3,
            ContractMonth::April => 4,
            ContractMonth::May => 5,
            ContractMonth::June => 6,
            ContractMonth::July => 7,
            ContractMonth::August => 8,
            ContractMonth::September => 9,
            ContractMonth::October => 10,
            ContractMonth::November => 11,
            ContractMonth::December => 12,
        }
    }
}

/// Calculate the front month contract for a given futures symbol
pub fn get_front_month_contract(symbol: &str) -> Result<(String, String)> {
    let today = Local::now().date_naive();
    
    // Get the appropriate expiry based on the symbol
    let (year, month, expiry_date) = match symbol {
        "ES" | "NQ" => {
            // E-mini S&P 500 and Nasdaq-100 futures expire on the third Friday of the contract month
            get_quarterly_expiry(today)
        }
        "CL" => {
            // Crude Oil futures expire on the third business day before the 25th
            get_monthly_expiry(today, 25, 3)
        }
        "GC" => {
            // Gold futures expire on the third last business day of the contract month
            get_monthly_expiry_end_of_month(today, 3)
        }
        _ => {
            // Default to quarterly contracts
            get_quarterly_expiry(today)
        }
    };
    
    // Format expiry as YYYYMMDD
    let expiry = format!("{:04}{:02}{:02}", year, month, expiry_date.day());
    
    // Format contract month as YYYYMM
    let contract_month = format!("{:04}{:02}", year, month);
    
    Ok((expiry, contract_month))
}

/// Get the next quarterly expiry (Mar, Jun, Sep, Dec)
fn get_quarterly_expiry(current_date: NaiveDate) -> (i32, u32, NaiveDate) {
    let quarterly_months = vec![3, 6, 9, 12];
    
    let current_year = current_date.year();
    let current_month = current_date.month();
    
    // Find the next quarterly month
    for &month in &quarterly_months {
        if month > current_month {
            let expiry = get_third_friday(current_year, month);
            if expiry > current_date {
                return (current_year, month, expiry);
            }
        }
    }
    
    // If no quarterly month found in current year, use March of next year
    let next_year = current_year + 1;
    let expiry = get_third_friday(next_year, 3);
    (next_year, 3, expiry)
}

/// Get the third Friday of a given month
fn get_third_friday(year: i32, month: u32) -> NaiveDate {
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    
    // Find the first Friday
    let days_until_friday = match first_day.weekday() {
        Weekday::Mon => 4,
        Weekday::Tue => 3,
        Weekday::Wed => 2,
        Weekday::Thu => 1,
        Weekday::Fri => 0,
        Weekday::Sat => 6,
        Weekday::Sun => 5,
    };
    
    // Third Friday is first Friday + 14 days
    first_day + chrono::Duration::days((days_until_friday + 14) as i64)
}

/// Get monthly expiry based on a specific day minus business days
fn get_monthly_expiry(current_date: NaiveDate, day_of_month: u32, business_days_before: u32) -> (i32, u32, NaiveDate) {
    let current_year = current_date.year();
    let current_month = current_date.month();
    
    // Try current month first
    if let Some(expiry) = calculate_business_days_before(current_year, current_month, day_of_month, business_days_before) {
        if expiry > current_date {
            return (current_year, current_month, expiry);
        }
    }
    
    // Try next month
    let (next_year, next_month) = if current_month == 12 {
        (current_year + 1, 1)
    } else {
        (current_year, current_month + 1)
    };
    
    if let Some(expiry) = calculate_business_days_before(next_year, next_month, day_of_month, business_days_before) {
        return (next_year, next_month, expiry);
    }
    
    // Fallback
    (next_year, next_month, NaiveDate::from_ymd_opt(next_year, next_month, 15).unwrap())
}

/// Get monthly expiry based on business days from end of month
fn get_monthly_expiry_end_of_month(current_date: NaiveDate, business_days_before: u32) -> (i32, u32, NaiveDate) {
    let current_year = current_date.year();
    let current_month = current_date.month();
    
    // Try current month first
    let last_day = get_last_day_of_month(current_year, current_month);
    if let Some(expiry) = calculate_business_days_before(current_year, current_month, last_day, business_days_before) {
        if expiry > current_date {
            return (current_year, current_month, expiry);
        }
    }
    
    // Try next month
    let (next_year, next_month) = if current_month == 12 {
        (current_year + 1, 1)
    } else {
        (current_year, current_month + 1)
    };
    
    let last_day = get_last_day_of_month(next_year, next_month);
    if let Some(expiry) = calculate_business_days_before(next_year, next_month, last_day, business_days_before) {
        return (next_year, next_month, expiry);
    }
    
    // Fallback
    (next_year, next_month, NaiveDate::from_ymd_opt(next_year, next_month, 15).unwrap())
}

/// Get the last day of a month
fn get_last_day_of_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30, // Should never happen
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Calculate a date that is N business days before a given day
fn calculate_business_days_before(year: i32, month: u32, day: u32, business_days: u32) -> Option<NaiveDate> {
    let target_date = NaiveDate::from_ymd_opt(year, month, day)?;
    let mut current_date = target_date;
    let mut business_days_count = 0;
    
    while business_days_count < business_days {
        current_date = current_date.pred_opt()?;
        
        // Skip weekends
        match current_date.weekday() {
            Weekday::Sat | Weekday::Sun => continue,
            _ => business_days_count += 1,
        }
    }
    
    Some(current_date)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contract_month_conversion() {
        assert_eq!(ContractMonth::March.code(), 'H');
        assert_eq!(ContractMonth::December.code(), 'Z');
        assert_eq!(ContractMonth::from_month(3).unwrap(), ContractMonth::March);
        assert_eq!(ContractMonth::June.to_month(), 6);
    }
    
    #[test]
    fn test_third_friday() {
        // Test known third Fridays
        assert_eq!(get_third_friday(2025, 3), NaiveDate::from_ymd_opt(2025, 3, 21).unwrap());
        assert_eq!(get_third_friday(2025, 6), NaiveDate::from_ymd_opt(2025, 6, 20).unwrap());
        assert_eq!(get_third_friday(2025, 9), NaiveDate::from_ymd_opt(2025, 9, 19).unwrap());
        assert_eq!(get_third_friday(2025, 12), NaiveDate::from_ymd_opt(2025, 12, 19).unwrap());
    }
    
    #[test]
    fn test_get_front_month_contract() {
        // Test ES and NQ futures
        let (expiry, month) = get_front_month_contract("ES").unwrap();
        
        // Verify format
        assert_eq!(expiry.len(), 8); // YYYYMMDD
        assert_eq!(month.len(), 6);  // YYYYMM
        
        // Verify the expiry is in the future
        let today = Local::now().date_naive();
        let expiry_date = NaiveDate::parse_from_str(&expiry, "%Y%m%d").unwrap();
        assert!(expiry_date > today, "Expiry date should be in the future");
        
        // Test NQ futures
        let (nq_expiry, nq_month) = get_front_month_contract("NQ").unwrap();
        assert_eq!(nq_expiry.len(), 8);
        assert_eq!(nq_month.len(), 6);
    }
}