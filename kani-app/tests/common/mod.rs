#![allow(clippy::unwrap_used, dead_code)]
//! Re-exports shared fixtures from the kani-shared-test crate.

pub use kani_shared_test::*;

/// Starts a local HTTP/1.1 server that responds to every GET with a small fake JPEG.
/// Returns the bound port. The server runs until the test process exits.
pub async fn start_mock_page_server() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const FAKE_BYTES: &[u8] = b"\xff\xd8\xff\xe0FAKE_IMAGE_DATA";

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    FAKE_BYTES.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(FAKE_BYTES).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    port
}
