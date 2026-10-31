//! Minimal integration smoke test: MerkleClient -> running Merkle service.
//!
//! Run: cargo run --example merkle_smoke
//! Requires the Merkle service listening on http://localhost:4003.

use zkpool_backend::tree::MerkleClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = MerkleClient::new("http://localhost:4003");

    let one = "0".repeat(63) + "1";
    let two = "0".repeat(63) + "2";

    let h = client.hash_2(&one, &two).await?;
    println!("hash_2(1, 2) = {}", h);

    let expected = "299bfccd7daf3c917e51291383929049ec0eaed800af245056cbf135f7dea636";
    if h != expected {
        anyhow::bail!("MISMATCH: expected {}, got {}", expected, h);
    }
    println!("OK");

    Ok(())
}
