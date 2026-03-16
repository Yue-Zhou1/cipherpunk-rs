pub fn trigger_unsafe_signature_path(sig: &[u8]) {
    unsafe_signature_verify(sig);
}

fn unsafe_signature_verify(_sig: &[u8]) {}
