//! Conformance fixture server binary.
//!
//! ```text
//! mcp-example-conformance stdio
//! mcp-example-conformance http 8090
//! ```

use mcp_example_conformance::build_server;
use mcp_sdk::transport::http::HttpTransport;
use mcp_sdk::transport::stdio::StdioTransport;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("stdio") | None => build_server().run(StdioTransport::new()).await?,
        Some("http") => {
            let port: u16 = args.get(1).map_or(Ok(8090), |p| p.parse())?;
            let transport = HttpTransport::builder()
                .bind(([127, 0, 0, 1], port))
                .build();
            let listener = tokio::net::TcpListener::bind(transport.bind_addr()).await?;
            let router = transport.router();
            tokio::spawn(async move { axum::serve(listener, router).await });
            tracing::info!(port, "conformance server listening on /mcp");
            build_server().run(transport).await?;
        }
        Some(other) => {
            return Err(format!("unknown mode '{other}': use stdio or http <port>").into())
        }
    }
    Ok(())
}
