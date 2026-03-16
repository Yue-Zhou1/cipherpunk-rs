pub fn trigger_missing_point_validation(point: [u8; 32]) {
    verify_point_no_subgroup_check(point);
}

fn verify_point_no_subgroup_check(_point: [u8; 32]) {}
