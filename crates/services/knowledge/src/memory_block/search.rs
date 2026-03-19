pub fn top_k_cosine(
    vectors: &[f32],
    dimensions: usize,
    query: &[f32],
    k: usize,
) -> Vec<(usize, f32)> {
    if k == 0 || dimensions == 0 || query.len() != dimensions {
        return vec![];
    }

    let mut scored = vectors
        .chunks_exact(dimensions)
        .enumerate()
        .map(|(index, row)| {
            let score = row
                .iter()
                .zip(query.iter())
                .map(|(left, right)| left * right)
                .sum::<f32>();
            (index, score)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|(left_index, left_score), (right_index, right_score)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| left_index.cmp(right_index))
    });
    scored.truncate(k.min(scored.len()));
    scored
}
