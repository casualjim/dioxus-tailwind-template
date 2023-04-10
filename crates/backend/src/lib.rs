use axum::{response::IntoResponse, Json};
use hyper::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use thiserror::Error;

pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
  #[error("unknown error")]
  Unknown,
  #[error("Corrupt Session")]
  CorruptSession,
  #[error("database: {0}")]
  Pgx(#[from] sqlx::Error),
  #[error("serde: {0}")]
  Serde(#[from] serde_json::Error),
}

impl IntoResponse for Error {
  fn into_response(self) -> axum::response::Response {
    let status_code = match &self {
      Error::CorruptSession => StatusCode::INTERNAL_SERVER_ERROR,
      Error::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
      Error::Pgx(sqlx::Error::Database(db_err)) if db_err.code() == Some("23505".into()) => {
        StatusCode::CONFLICT
      }
      Error::Pgx(sqlx::Error::RowNotFound) => StatusCode::NOT_FOUND,
      Error::Pgx(_e) => StatusCode::INTERNAL_SERVER_ERROR,
      Error::Serde(_e) => StatusCode::BAD_REQUEST,
    };
    (status_code, Json(json!({"message": self.to_string()}))).into_response()
  }
}

#[derive(Clone)]
pub struct AppState {
  pool: PgPool,
}

impl AppState {
  pub async fn new<S: Into<String>>(database_url: S) -> Result<Self> {
    let pool = PgPool::connect(&(database_url).into()).await?;

    Ok(AppState { pool })
  }

  pub fn pool(&self) -> &PgPool {
    &self.pool
  }
}
