use std::sync::{Arc, OnceLock};

use anyhow::{bail, Context, Result};

use bytes::Bytes;
use http_body_util::Empty;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;

use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{self, pki_types::ServerName},
    TlsConnector,
};

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl Stream for TcpStream {}
impl Stream for TlsStream<TcpStream> {}

static TLS_CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();

#[inline]
fn tls_config() -> Arc<rustls::ClientConfig> {
    TLS_CONFIG
        .get_or_init(|| {
            let _ = rustls::crypto::ring::default_provider()
                .install_default()
                .expect("failed to instal default crypto provider");

            let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.to_owned());
            let tls = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(tls)
        })
        .clone()
}

pub async fn request(url: &hyper::Uri) -> Result<Response<hyper::body::Incoming>> {
    enum Protocol {
        HTTP,
        HTTPS,
    }

    let protocol = match url.scheme_str() {
        Some("http") => Protocol::HTTP,
        Some("https") => Protocol::HTTPS,

        Some(protocol) => bail!("invalid protocol: {protocol}"),
        None => bail!("no protocol"),
    };

    let host = url.host().context("no host in url")?.to_owned();
    let port = url.port_u16().unwrap_or_else(|| match protocol {
        Protocol::HTTP => 80,
        Protocol::HTTPS => 443,
    });
    let addr = format!("{host}:{port}");

    let tcp_stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("failed to connect to: {addr}"))?;

    let stream: Box<dyn Stream> = match protocol {
        Protocol::HTTPS => {
            let domain = ServerName::try_from(host)?;
            let connector = TlsConnector::from(tls_config());
            let tls_stream = connector.connect(domain, tcp_stream).await?;
            Box::new(tls_stream)
        }
        Protocol::HTTP => Box::new(tcp_stream),
    };

    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            eprintln!("Connection failed: {:?}", err);
        }
    });

    let authority = url.authority().context("failed to get authority")?.clone();

    let req = Request::builder()
        .uri(
            url.path_and_query()
                .map(|p| p.as_str())
                .unwrap_or_else(|| url.path()),
        )
        .header(hyper::header::HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;

    Ok(sender.send_request(req).await?)
}
