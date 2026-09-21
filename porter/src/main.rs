mod jev;
mod notion;
mod openrouter;
mod llm;
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
    Route { #[arg(long)] agent: String, #[arg(long)] task: String },
}
#[derive(Subcommand)]
enum KeyOp {
    Set { agent: String, provider: String },
    Has { agent: String, provider: String },
    List,
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
