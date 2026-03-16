pub fn trigger_nonce_reuse() {
    let key = [7u8; 32];
    let nonce = 0u64;
    let plaintext = b"secret";
    aead_encrypt(key, nonce, plaintext);
}

fn aead_encrypt(_key: [u8; 32], _nonce: u64, _plaintext: &[u8]) {}
