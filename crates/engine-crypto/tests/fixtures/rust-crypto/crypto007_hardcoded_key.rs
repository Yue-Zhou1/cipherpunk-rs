pub fn trigger_hardcoded_key() {
    let key = [9u8; 32];
    hardcoded_key_material(key);
}

fn hardcoded_key_material(_key: [u8; 32]) {}
