// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Proprietary software — see LICENSE in the repository root.
// https://zyvor.dev · info@zyvor.dev

//! Sales / packaging tooling for Ed25519-signed Atlas trial/license tokens. Same shape as
//! `veyron::bin::trial-tool` — see that binary and `atlas-license`'s module docs for the design.
//!
//! ```text
//! cargo run -p atlas-license-tool -- keygen
//! cargo run -p atlas-license-tool -- issue --who "Acme Corp" --days 30 -o trial.token
//! ```
//!
//! Private key stays with Zyvor sales. Only the public key is embedded in the gateway
//! (`atlas_license::LICENSE_PUBLIC_KEY_B64`). Never ship `secrets/` or this binary in a
//! customer package.

use atlas_license::{LicenseClaims, PRODUCT_TAG};
use base64::Engine;
use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "atlas-license-tool", about = "Issue Ed25519-signed Atlas trial/license tokens")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a new Ed25519 keypair. Paste the public key into
    /// crates/atlas-license/src/lib.rs's LICENSE_PUBLIC_KEY_B64.
    Keygen {
        #[arg(long, default_value = "secrets/atlas-license-ed25519.pkcs8")]
        out_private: PathBuf,
    },
    /// Sign a trial/license token (requires the private PKCS8 from keygen).
    Issue {
        #[arg(long)]
        who: String,
        #[arg(long, default_value_t = 30)]
        days: i64,
        #[arg(long, default_value = "secrets/atlas-license-ed25519.pkcs8")]
        private_key: PathBuf,
        #[arg(short, long, default_value = "trial.token")]
        output: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Keygen { out_private } => {
            if let Some(parent) = out_private.parent() {
                fs::create_dir_all(parent)?;
            }
            let rng = SystemRandom::new();
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| anyhow::anyhow!("ed25519 keygen failed"))?;
            fs::write(&out_private, pkcs8.as_ref())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&out_private)?.permissions();
                perms.set_mode(0o600);
                fs::set_permissions(&out_private, perms)?;
            }
            let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
                .map_err(|e| anyhow::anyhow!("parse pkcs8: {e}"))?;
            let pub_b64 =
                base64::engine::general_purpose::STANDARD.encode(pair.public_key().as_ref());
            println!("Wrote private PKCS8 → {}", out_private.display());
            println!("Paste into crates/atlas-license/src/lib.rs:\n");
            println!("pub const LICENSE_PUBLIC_KEY_B64: &str = \"{pub_b64}\";");
            println!("\nRe-issue any active trials after rotating.");
        }
        Cmd::Issue { who, days, private_key, output } => {
            let pkcs8 = fs::read(&private_key)
                .map_err(|e| anyhow::anyhow!("read {}: {e}", private_key.display()))?;
            let now = Utc::now();
            let claims = LicenseClaims {
                sub: who.clone(),
                iat: now.timestamp(),
                exp: (now + Duration::days(days)).timestamp(),
                product: PRODUCT_TAG.to_string(),
            };
            let key = EncodingKey::from_ed_der(&pkcs8);
            let token = encode(&Header::new(Algorithm::EdDSA), &claims, &key)?;
            fs::write(&output, format!("{token}\n"))?;
            println!(
                "Issued license for \"{who}\" → {} (expires in {days} days)",
                output.display()
            );
            println!("Set ATLAS_TRIAL_TOKEN to this file's contents, or ship the file as ATLAS_TRIAL_TOKEN_FILE.");
        }
    }
    Ok(())
}
