mod jev;
mod notion;
mod openrouter;
mod llm;
mod gateway;
mod supabase;
mod cloudflare;
mod deepseek;
mod proxy;
mod ui;
mod vault;
use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(name = "porter", about = "Local API holder + Jev-gated router")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand)]
enum Cmd {
    Serve { #[arg(long, default_value_t = 8819)] port: u16 },
    Key { #[command(subcommand)] op: KeyOp },
    Jev { #[command(subcommand)] op: JevOp },
    Token { #[command(subcommand)] op: TokenOp },
    Route { #[arg(long)] agent: String, #[arg(long)] task: String },
}
#[derive(Subcommand)]
enum KeyOp {
    Set { agent: String, provider: String },
    Has { agent: String, provider: String },
    List,
    Clear { agent: String, provider: String },
}
#[derive(Subcommand)]
enum TokenOp {
    Issue { agent: String },
    List,
    Revoke { agent: String, prefix: String },
}

#[derive(Subcommand)]
enum JevOp {
    Check { #[arg(long, default_value = "codex")] agent: String },
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { port } => {
            let app = proxy::router("codex".into());
            let addr = format!("127.0.0.1:{port}");
            println!("porter on {addr}");
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
        }
        Cmd::Key { op } => match op {
            KeyOp::Set { agent, provider } => {
                let v = std::env::var("VAULT_VALUE").unwrap_or_default();
                if v.trim().is_empty() {
                    anyhow::bail!("set VAULT_VALUE=... then retry");
                }
                vault::set(&agent, &provider, v.trim())?;
                println!("stored {}/{}", agent.to_lowercase(), provider.to_lowercase());
            }
            KeyOp::Has { agent, provider } => {
                println!("{}", if vault::has(&agent, &provider) { "yes" } else { "no" });
            }
            KeyOp::List => {
                for n in vault::list_names() { println!("{n}"); }
            }
            KeyOp::Clear { agent, provider } => {
                println!("{}", if vault::clear(&agent, &provider)? { "cleared" } else { "absent" });
            }
        },
        Cmd::Jev { op } => match op {
            JevOp::Check { agent } => {
                let Some(k) = vault::get(&agent, "typesafe") else {
                    anyhow::bail!("no jev key for {agent}");
                };
                let v = jev::check(&k).await?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        },
        Cmd::Token { op } => match op {
            TokenOp::Issue { agent } => {
                let tok = vault::issue_token(&agent).map_err(|e| anyhow::anyhow!(e))?;
                println!("{tok}");
                eprintln!("shown once — store it as the harness bearer token, then forget it");
            }
            TokenOp::List => {
                for (a, p) in vault::list_tokens() {
                    println!("{a}/{p}");
                }
            }
            TokenOp::Revoke { agent, prefix } => {
                println!(
                    "{}",
                    if vault::revoke_token(&agent, &prefix).map_err(|e| anyhow::anyhow!(e))? {
                        "revoked"
                    } else {
                        "absent"
                    }
                );
            }
        }
        Cmd::Route { agent, task } => {
            let Some(k) = vault::get(&agent, "typesafe") else {
                anyhow::bail!("no jev key for {agent}");
            };
            let v = jev::route(&k, &agent, &task).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
    }
    Ok(())
}
