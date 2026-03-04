use audit_agent_core::output::FindingCounts;

#[test]
fn risk_score_matches_expected_table() {
    let cases = [
        (
            FindingCounts {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
                observation: 0,
            },
            100,
        ),
        (
            FindingCounts {
                critical: 1,
                high: 0,
                medium: 0,
                low: 0,
                observation: 0,
            },
            75,
        ),
        (
            FindingCounts {
                critical: 0,
                high: 2,
                medium: 1,
                low: 3,
                observation: 4,
            },
            59,
        ),
        (
            FindingCounts {
                critical: 4,
                high: 0,
                medium: 0,
                low: 0,
                observation: 1,
            },
            0,
        ),
    ];

    for (counts, expected) in cases {
        assert_eq!(counts.risk_score(), expected);
    }
}
