//! Raw HTTP/1.1 wire helpers — one job: write responses on a raw TCP stream. The control
//! plane is hand-rolled on tokio rather than built on an HTTP framework; this is its whole
//! response layer.

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

/// Answer a CORS preflight (`OPTIONS`) with the methods/headers the client needs.
pub(super) async fn write_preflight(stream: &mut TcpStream) -> crate::Result<()> {
    let header = "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Access-Control-Max-Age: 86400\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\r\n";
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

pub(super) async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> crate::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

pub(super) fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
