use algotrading::futures_utils::get_front_month_contract;

fn main() {
    println!("Current front-month futures contracts:");

    for symbol in &["ES", "NQ", "CL", "GC"] {
        match get_front_month_contract(symbol) {
            Ok((expiry, contract_month)) => {
                println!(
                    "{}: expiry={}, contract_month={}",
                    symbol, expiry, contract_month
                );
            }
            Err(e) => {
                println!("{}: Error - {}", symbol, e);
            }
        }
    }
}
