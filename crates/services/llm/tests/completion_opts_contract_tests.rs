#[test]
fn completion_opts_default_matches_core_contract_default() {
    let core_default = audit_agent_core::llm::CompletionOpts::default();
    let service_default = llm::CompletionOpts::default();

    assert_eq!(
        service_default.temperature_millis, core_default.temperature_millis,
        "temperature defaults should stay in sync across core and service contracts"
    );
    assert_eq!(
        service_default.max_tokens, core_default.max_tokens,
        "token defaults should stay in sync across core and service contracts"
    );
}
