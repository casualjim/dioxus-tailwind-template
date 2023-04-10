
#[derive(Clone)]
pub struct AppState {
  pool: PgPool,
}

impl AppState {
  pub async fn new<S: Into<String>>(
    database_url: S,
  ) -> Result<Self> {
    let pool = PgPool::connect(&(database_url).into()).await?;

    Ok(AppState {
      pool,
    })
  }

  pub fn pool(&self) -> &PgPool {
    &self.pool
  }
}
