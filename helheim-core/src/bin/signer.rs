use anyhow::Result;
use clap::{Parser, Subcommand};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::fs;

use colored::*;

#[derive(Parser)]
#[command(name = "signer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new Keypair (private.key + public.key)
    GenKey,
    /// Sign a file using private.key -> generates file.sig
    Sign {
        path: String,
        #[arg(short, long, default_value = "private.key")]
        key: String,
    },
    /// Show the Public Key (Rust Array Format) from private.key
    ShowKey {
        #[arg(short, long, default_value = "private.key")]
        key: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenKey => {
            println!("🔑 Generating Helheim Keypair (Ed25519)...");
            let rng = SystemRandom::new();
            let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| anyhow::anyhow!("Failed to generate PKCS8 bytes"))?;
            let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
                .map_err(|_| anyhow::anyhow!("Failed to create KeyPair from PKCS8"))?;

            fs::write("private.key", pkcs8_bytes.as_ref())?;
            fs::write("public.key", key_pair.public_key().as_ref())?;

            println!("✅ Keys Generated:");
            println!("   - private.key (KEEP SECRET!)");
            println!("   - public.key (Embed this in the binary)");
        }
        Commands::Sign { path, key } => {
            println!("🔏 Signing '{}' with '{}'...", path, key);

            // Load Key
            let key_bytes =
                fs::read(&key).map_err(|_| anyhow::anyhow!("Private key not found!"))?;
            let key_pair = Ed25519KeyPair::from_pkcs8(&key_bytes)
                .map_err(|_| anyhow::anyhow!("Invalid Private Key"))?;

            // Load Data
            let data = fs::read(&path)?;

            // Sign
            let sig = key_pair.sign(&data);
            let sig_path = format!("{}.sig", path);

            fs::write(&sig_path, sig.as_ref())?;
            println!("✅ Signature wrote to: {}", sig_path.green().bold());
        }
        Commands::ShowKey { key } => {
            let key_bytes =
                fs::read(&key).map_err(|_| anyhow::anyhow!("Private key not found!"))?;
            let key_pair = Ed25519KeyPair::from_pkcs8(&key_bytes)
                .map_err(|_| anyhow::anyhow!("Invalid Private Key"))?;

            let pub_key = key_pair.public_key();
            println!("🔑 Public Key (Rust Format):");
            print!("const HELHEIM_MASTER_KEY: [u8; 32] = [\n    ");
            for (i, byte) in pub_key.as_ref().iter().enumerate() {
                print!("0x{:02x}, ", byte);
                if (i + 1) % 8 == 0 && i != 31 {
                    print!("\n    ");
                }
            }
            println!("\n];");
        }
    }
    Ok(())
}
