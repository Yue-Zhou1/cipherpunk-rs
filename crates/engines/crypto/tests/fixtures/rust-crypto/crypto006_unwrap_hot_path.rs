pub fn trigger_unwrap_in_hot_path() {
    unwrap_crypto_result(Result::<u64, &'static str>::Ok(1));
}

fn unwrap_crypto_result(result: Result<u64, &'static str>) -> u64 {
    result.unwrap_or_default()
}
