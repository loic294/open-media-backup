use omb_hash_server::{config::Config, router, HashServer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = Config::from_env()?;
    let address = format!("0.0.0.0:{}", config.port);
    let server = HashServer::new(
        config.id.clone(),
        config.name,
        config.token.clone(),
        config.roots,
        2,
    )?;
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!(
        "omb-hash-server {} listening on {address}; id: {}",
        omb_hash_server::VERSION,
        config.id
    );
    println!("Pairing token: {}", config.token);
    axum::serve(
        listener,
        router(server).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
}
