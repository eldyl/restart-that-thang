use super::handle_request;
use tokio::net::TcpListener;

pub async fn health(listener: TcpListener) -> anyhow::Result<()> {
    log::info!(
        "Health check server running at {}",
        listener.local_addr().unwrap()
    );

    while let Ok((socket, _)) = listener.accept().await {
        tokio::spawn(handle_request(socket));
    }

    Ok(())
}
