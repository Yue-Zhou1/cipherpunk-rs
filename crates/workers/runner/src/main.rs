use anyhow::Context;
use worker_protocol::WorkerExecutionRequest;
use worker_runner::WorkerRunner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let input = std::env::args()
        .nth(1)
        .context("missing worker request json path")?;
    let payload =
        std::fs::read_to_string(&input).with_context(|| format!("read worker request {input}"))?;
    let request: WorkerExecutionRequest =
        serde_json::from_str(&payload).context("parse worker request json")?;
    let result = WorkerRunner::execute(request).await;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
