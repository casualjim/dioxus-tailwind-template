use anyhow::{anyhow, Result};
use axum::body::Body;
use axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
use axum::extract::Host;
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::response::Redirect;
use axum::BoxError;
use axum::Router;

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use axum_sessions::async_session::CookieStore;
use axum_sessions::{SameSite, SessionLayer};

use clap::{Parser, ValueEnum};

use hyper::Method;
use listenfd::ListenFd;
use rand::Rng;
use rustls::{Certificate, PrivateKey, ServerConfig};

use rustls_pemfile::{certs, pkcs8_private_keys};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::net::{Ipv6Addr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

use time::ext::NumericalStdDuration;
use tokio_stream::StreamExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use {{project-name}}d::AppState;

// We use jemalloc as it produces better performance.
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[derive(Parser, Debug)]
pub struct Args {
  /// Domains
  #[clap(long)]
  domains: Vec<String>,

  /// Cache directory
  #[clap(long, value_hint = clap::ValueHint::DirPath)]
  cache: Option<PathBuf>,

  /// The private key when tls-mode is keypair
  #[clap(long, value_hint = clap::ValueHint::DirPath)]
  tls_key: Option<PathBuf>,

  /// The public key when tls-mode is keypair
  #[clap(long, value_hint = clap::ValueHint::DirPath)]
  tls_cert: Option<PathBuf>,

  /// The port to listen on for secure traffic
  #[clap(long, default_value = "443")]
  https_port: u16,

  /// The port to listen on for unecrypted traffic
  #[clap(long, default_value = "80")]
  http_port: u16,

  /// The directory containing the static files to serve
  #[clap(long, value_hint = clap::ValueHint::DirPath)]
  static_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
  human_panic::setup_panic!();

  tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::new(
      std::env::var("RUST_LOG").unwrap_or_else(|_| "franklin=debug,franklind=debug".into()),
    ))
    .with(tracing_subscriber::fmt::layer())
    .init();

  let database_url =
    ::dotenvy::var("DATABASE_URL").expect("expected $DATABASE_URL to be set");
  info!("Connecting to database with {}.", &database_url);

  let args = Args::parse();

  let handle = Handle::new();
  let hh = handle.clone();

  // Spawn a task to gracefully shutdown server.
  tokio::spawn(async move {
    tokio::signal::ctrl_c().await.unwrap();
    graceful_shutdown(hh).await;
  });

  let app_state = AppState::new(database_url)
    .await?
  sqlx::migrate!().run(app_state.pool()).await?;

  // this will enable us to keep application running during recompile: systemfd --no-pid -s http::8080 -s https::8443 -- cargo watch -x run
  let mut listenfd = ListenFd::from_env();

  if args.tls_key.as_ref().ss_dome() || args.tls_cert.as_ref().is_some() {
    let http_listener = listenfd.take_tcp_listener(0).unwrap();
    let https_listener = listenfd.take_tcp_listener(1).unwrap();
    tokio::spawn(redirect_http_to_https(
      handle.clone(),
      args.http_port,
      args.https_port,
      http_listener,
    ));

    debug!(
        key = ?args.tls_key,
        cert = ?args.tls_cert,
        "Using keypair files"
    );

    serve_keypair(app_state, &args, handle, https_listener).await?
  } else {
    let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, args.http_port));

    axum_server::bind(addr)
      .handle(handle)
      .serve(make_app_async(app_state, &args.static_dir, false).await)
      .await?
  }

  Ok(())
}

async fn serve_keypair(
  app_state: AppState,
  args: &Args,
  handle: Handle,
  listener: Option<TcpListener>,
) -> Result<()> {
  if let (Some(key), Some(cert)) = (args.tls_key.as_ref(), args.tls_cert.as_ref()) {
    let mut key_reader = BufReader::new(File::open(key).unwrap());
    let mut cert_reader = BufReader::new(File::open(cert).unwrap());

    let key = PrivateKey(pkcs8_private_keys(&mut key_reader).unwrap().remove(0));
    let certs = certs(&mut cert_reader).unwrap().into_iter().map(Certificate).collect();

    let mut config = ServerConfig::builder()
      .with_safe_defaults()
      .with_no_client_auth()
      .with_single_cert(certs, key)
      .expect("bad certificate/key");
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    if let Some(l) = listener {
      debug!("https (listenfd) listening on {}", l.local_addr().unwrap());
      axum_server::from_tcp_rustls(l, RustlsConfig::from_config(Arc::new(config)))
        .handle(handle)
        .serve(make_app_async(app_state, &args.static_dir, true).await)
        .await
        .map_err(|e| anyhow!("{:?}", e))?;
    } else {
      let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, args.https_port));
      debug!("https listening on {}", addr);
      axum_server::bind_rustls(addr, RustlsConfig::from_config(Arc::new(config)))
        .handle(handle)
        .serve(make_app_async(app_state, &args.static_dir, true).await)
        .await
        .map_err(|e| anyhow!("{:?}", e))?;
    }
    Ok(())
  } else {
    Err(anyhow!("both tls-key and tls-cert are required"))
  }
}

async fn redirect_http_to_https(
  handle: Handle,
  http_port: u16,
  https_port: u16,
  listener: Option<TcpListener>,
) {
  fn make_https(
    host: String,
    uri: Uri,
    http_port: &str,
    https_port: &str,
  ) -> Result<Uri, BoxError> {
    let mut parts = uri.into_parts();

    parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

    if parts.path_and_query.is_none() {
      parts.path_and_query = Some("/".parse().unwrap());
    }

    let https_host = host.replace(http_port, https_port);
    parts.authority = Some(https_host.parse()?);

    Ok(Uri::from_parts(parts)?)
  }

  let (httpp, httpsp) = (http_port.to_string(), https_port.to_string());
  let redirect = move |Host(host): Host, uri: Uri| async move {
    match make_https(host, uri, &httpp, &httpsp) {
      Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
      Err(error) => {
        warn!(%error, "failed to convert URI to HTTPS");
        Err(StatusCode::BAD_REQUEST)
      }
    }
  };

  if let Some(l) = listener {
    debug!(
      "http redirect (listenfd) listening on {}",
      l.local_addr().unwrap()
    );
    axum_server::from_tcp(l).handle(handle).serve(redirect.into_make_service()).await.unwrap();
  } else {
    let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, http_port));
    debug!("http redirect listening on {}", addr);

    axum_server::bind(addr).handle(handle).serve(redirect.into_make_service()).await.unwrap();
  }
}

async fn make_app_async(
  app_state: AppState,
  static_dir: &PathBuf,
  is_secure: bool,
) -> IntoMakeServiceWithConnectInfo<Router<(), Body>, SocketAddr> {
  //Configure cookie based sessions
  let store = CookieStore::new();
  let secret = rand::thread_rng().gen::<[u8; 128]>(); // MUST be at least 64 bytes!
  let session_layer = SessionLayer::new(store, &secret)
    .with_cookie_name("{{project-name}}_session")
    .with_same_site_policy(SameSite::Lax)
    .with_secure(is_secure);

  let static_dir_str = static_dir.to_string_lossy();
  let serve_dir = ServeDir::new(static_dir)
    .fallback(ServeFile::new(format!("{static_dir_str}/index.html")));

  Router::new()
    .fallback_service(serve_dir)
    .layer(
      CorsLayer::new()
        .allow_origin([
          "http://localhost:8080".parse::<HeaderValue>().unwrap(),
          "https://localhost:8443".parse::<HeaderValue>().unwrap(),
        ])
        .allow_headers(Any)
        .allow_methods([
          Method::GET,
          Method::POST,
          Method::OPTIONS,
          Method::PUT,
          Method::DELETE,
        ]),
    )
    .layer(session_layer)
    .layer(TraceLayer::new_for_http())
    .into_make_service_with_connect_info()
}

async fn graceful_shutdown(handle: Handle) {
  debug!("received graceful shutdown signal");
  handle.graceful_shutdown(Some(10.std_seconds()));
  loop {
    debug!("alive connections: {}", handle.connection_count());
    if handle.connection_count() == 0 {
      break;
    }
    sleep(Duration::from_secs(1)).await;
  }
}
