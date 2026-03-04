pub fn trigger_deterministic_rng() {
    let _rng = deterministic_rng_new(123456789);
}

fn deterministic_rng_new(_seed: u64) -> u64 {
    0
}
