use std::env;

use verified_domain_workspace_join::infrai_client::InfraiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let domain = env::var("COMPANY_DOMAIN")?;
    let proof = env::var("DOMAIN_PROOF")?;
    let client = InfraiClient::from_env()?;

    let added = client.add_domain(&domain).await?;
    client
        .upsert_txt(&added.zone_id, "_infrai-verification", &proof)
        .await?;
    let verified = client.verify_domain(&domain).await?;

    println!(
        "domain={} zone_id={} verified={}",
        domain, verified.zone_id, verified.verified
    );
    Ok(())
}
