use anyhow::Context;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub mod health;
pub use health::health;

pub(crate) async fn handle_request(mut socket: TcpStream) -> anyhow::Result<()> {
    let mut reader = BufReader::new(&mut socket);
    let mut request_line = String::new();

    reader
        .read_line(&mut request_line)
        .await
        .context("Bad request")?;

    let req_parts: Vec<&str> = request_line.split_whitespace().collect();

    if req_parts.len() < 3 {
        send_response(&mut socket, 400, "text/plain", "Bad Request").await?;
        return Ok(());
    }

    let method = req_parts[0];
    let path = req_parts[1];

    match (method, path) {
        ("GET", "/health") => send_response(&mut socket, 200, "text/plain", "Healthy\n").await?,
        ("GET", _) => send_response(&mut socket, 404, "text/plain", "Not Found\n").await?,
        _ => send_response(&mut socket, 400, "text/plain", "Bad Request\n").await?,
    }

    Ok(())
}

async fn send_response(
    socket: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> anyhow::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status,
        status_text,
        content_type,
        body.len(),
        body
    );

    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.shutdown().await;

    Ok(())
}
