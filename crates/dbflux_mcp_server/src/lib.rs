mod bootstrap;
mod connection_cache;
mod error_messages;
mod handlers;
mod server;
mod transport;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct McpServerArgs {
    pub client_id: String,
    pub config_dir: Option<PathBuf>,
}

pub fn run_mcp_server(args: McpServerArgs) -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let mut state = bootstrap::init(args.client_id.clone(), args.config_dir)
        .map_err(|e| anyhow::anyhow!("Failed to initialize MCP server: {}", e))?;

    log::info!("dbflux-mcp-server started, client_id={}", state.client_id);

    let mut reader = transport::stdin_reader();
    let mut writer = std::io::stdout();

    server::run(&mut state, &mut reader, &mut writer)?;

    Ok(())
}
