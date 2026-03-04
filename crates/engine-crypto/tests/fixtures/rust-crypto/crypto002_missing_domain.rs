pub fn trigger_missing_domain_separator() {
    let transcript = b"proof-transcript";
    transcript_hash_no_domain(transcript);
}

fn transcript_hash_no_domain(_input: &[u8]) {}
