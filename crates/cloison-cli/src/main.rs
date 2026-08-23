//! Binaire `cloison-cli` — outillage ops N3 du plan de contrôle CLOISON.
//!
//! Usage :
//! ```bash
//! cloison-cli --control-url http://127.0.0.1:8788 provision acme --nom "Acme SARL" --plan pro
//! cloison-cli token issue acme
//! cloison-cli token verify acme mn_xxxxxxxx
//! cloison-cli ledger check /tmp/ledger.jsonl --pubkey-file /tmp/control_pubkey.hex
//! ```
//!
//! Le clair `mn_` n'est affiché qu'à l'émission (jamais journalisé) ; `verify`
//! ne transmet que le hash (invariant I2). Aucun secret en URL ni en log.

use clap::Parser;
use cloison_cli::*;
use std::io::Read;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("ERREUR : {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let client = ControlClient::new(cli.control_url)?;
    match cli.command {
        Command::Provision(a) => provision(&client, a).await,
        Command::TenantGet(a) => tenant_get(&client, a).await,
        Command::TokenIssue(a) => token_issue(&client, a).await,
        Command::TokenRotate(a) => token_rotate(&client, a).await,
        Command::TokenRevoke(a) => token_revoke(&client, a).await,
        Command::TokenVerify(a) => token_verify(&client, a).await,
        Command::PolicySet(a) => policy_set(&client, a).await,
        Command::LicenseAdd(a) => license_add(&client, a).await,
        Command::LedgerRoot => ledger_root(&client).await,
        Command::LedgerCheck(a) => {
            verify_ledger_file(&a.ledger_file, a.pubkey_file.as_ref())?;
            Ok(())
        }
        Command::Stats(a) => stats(&client, a).await,
    }
}

/// POST /admin/tenants (tenant + licence) puis, optionnellement, émission d'un jeton.
async fn provision(client: &ControlClient, a: ProvisionArgs) -> Result<(), CliError> {
    let tenant: serde_json::Value = client
        .post(
            "/admin/tenants",
            &CreateTenantReq {
                id: a.tenant_id.clone(),
                nom_public: a.nom,
                plan: a.plan,
            },
        )
        .await?;
    println!("Tenant créé :");
    println!("{}", serde_json::to_string_pretty(&tenant)?);

    if a.issue_token {
        let issued: serde_json::Value = client
            .post(
                &format!("/admin/tenants/{}/tokens", a.tenant_id),
                &IssueTokenReq { scopes: Vec::new() },
            )
            .await?;
        print_issued(&issued);
    }
    Ok(())
}

/// GET /admin/tenants/{id}
async fn tenant_get(client: &ControlClient, a: TenantGetArgs) -> Result<(), CliError> {
    let v: serde_json::Value = client
        .get(&format!("/admin/tenants/{}", a.tenant_id))
        .await?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// POST /admin/tenants/{id}/tokens — le clair est affiché UNE fois.
async fn token_issue(client: &ControlClient, a: TokenIssueArgs) -> Result<(), CliError> {
    let issued: serde_json::Value = client
        .post(
            &format!("/admin/tenants/{}/tokens", a.tenant_id),
            &IssueTokenReq { scopes: a.scope },
        )
        .await?;
    print_issued(&issued);
    Ok(())
}

fn print_issued(issued: &serde_json::Value) {
    let token = issued.get("token").and_then(|t| t.as_str()).unwrap_or("");
    let id = issued.get("id").and_then(|t| t.as_str()).unwrap_or("");
    println!();
    println!(
        "Jeton émis (clair affiché UNE SEULE FOIS — à communiquer au client, jamais à logger) :"
    );
    println!("  id    : {id}");
    println!("  jeton : {token}");
    println!();
    println!("Clé composite pour l'interface IA :");
    println!("  Base URL : https://api.wonkom.ai/v1");
    println!("  Clé      : {token}.<clé_amont_du_client>");
}

/// POST /admin/tenants/{id}/rotate — l'ancien jeton passe en grâce.
async fn token_rotate(client: &ControlClient, a: TokenRotateArgs) -> Result<(), CliError> {
    let issued: serde_json::Value = client
        .post(
            &format!("/admin/tenants/{}/rotate", a.tenant_id),
            &RotateTokenReq {
                token_id: a.token_id,
            },
        )
        .await?;
    print_issued(&issued);
    Ok(())
}

/// DELETE /admin/tenants/{id}/tokens/{token_id} — révocation immédiate.
async fn token_revoke(client: &ControlClient, a: TokenRevokeArgs) -> Result<(), CliError> {
    let status = client
        .delete(&format!(
            "/admin/tenants/{}/tokens/{}",
            a.tenant_id, a.token_id
        ))
        .await?;
    println!("Jeton révoqué (HTTP {status}).");
    Ok(())
}

/// POST /v1/control/verify — seul le hash traverse le réseau.
async fn token_verify(client: &ControlClient, a: TokenVerifyArgs) -> Result<(), CliError> {
    let digest = token_hash(&a.token);
    // Le clair est oublié immédiatement après le hash.
    drop(a.token);
    let resp: serde_json::Value = client
        .post(
            "/v1/control/verify",
            &VerifyRequest {
                tenant_id: a.tenant_id,
                token_hash: digest,
            },
        )
        .await?;
    let valid = resp.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
    let version = resp.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    println!(
        "Jeton {} (tenant version {version}).",
        if valid { "VALIDE" } else { "INVALIDE" }
    );
    if !valid {
        return Err(CliError::Message("jeton rejeté par le contrôle".into()));
    }
    Ok(())
}

/// PUT /admin/tenants/{id}/policy — JSON depuis fichier ou stdin.
async fn policy_set(client: &ControlClient, a: PolicySetArgs) -> Result<(), CliError> {
    let json_policy = if a.json_file == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(&a.json_file)?
    };
    // Valide que c'est du JSON avant l'envoi (erreur locale, pas de corps en log).
    serde_json::from_str::<serde_json::Value>(&json_policy)
        .map_err(|e| CliError::Message(format!("politique : JSON invalide : {e}")))?;
    let v: serde_json::Value = client
        .put_json(
            &format!("/admin/tenants/{}/policy", a.tenant_id),
            &PutPolicyReq { json_policy },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// POST /admin/tenants/{id}/licenses
async fn license_add(client: &ControlClient, a: LicenseAddArgs) -> Result<(), CliError> {
    let v: serde_json::Value = client
        .post(
            &format!("/admin/tenants/{}/licenses", a.tenant_id),
            &AddLicenseReq {
                plan: a.plan,
                expires_at: a.expires_at,
            },
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// GET /v1/control/root
async fn ledger_root(client: &ControlClient) -> Result<(), CliError> {
    let v: serde_json::Value = client.get("/v1/control/root").await?;
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// Stats : version du jeton + racine du journal.
async fn stats(client: &ControlClient, a: StatsArgs) -> Result<(), CliError> {
    let version: serde_json::Value = client
        .get(&format!("/v1/control/version?tenant_id={}", a.tenant_id))
        .await?;
    let root: serde_json::Value = client.get("/v1/control/root").await?;
    println!("Tenant : {}", a.tenant_id);
    println!(
        "  version jeton : {}",
        version.get("version").and_then(|v| v.as_u64()).unwrap_or(0)
    );
    println!(
        "  seq journal   : {}",
        root.get("seq").and_then(|v| v.as_u64()).unwrap_or(0)
    );
    println!(
        "  root hash     : {}",
        root.get("root_hash").and_then(|v| v.as_str()).unwrap_or("")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn control_url_trailing_slash_normalized() {
        let c = ControlClient::new("http://x:8788/".into()).unwrap();
        assert_eq!(c.url("/v1/control/root"), "http://x:8788/v1/control/root");
    }
}
