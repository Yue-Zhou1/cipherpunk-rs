pub fn trigger_field_deserialization_issue(bytes: &[u8]) {
    let _value = deserialize_field_unchecked(bytes);
}

fn deserialize_field_unchecked(_bytes: &[u8]) -> u64 {
    42
}
