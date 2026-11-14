#[tokio::main]
async fn main() -> anyhow::Result<()> {
    emberwatch_server::serve().await
}
