use crate::engine::Snapshot;
use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone)]
pub enum Store {
    Memory(Arc<Mutex<Option<Snapshot>>>),
    File(PathBuf),
    Postgres(PgPool),
}

impl Store {
    pub fn memory() -> Self {
        Self::Memory(Arc::new(Mutex::new(None)))
    }
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    pub async fn postgres(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await
            .context("could not connect to PostgreSQL")?;
        sqlx::migrate!("../../migrations").run(&pool).await?;
        Ok(Self::Postgres(pool))
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::File(_) => "file",
            Self::Postgres(_) => "postgres",
        }
    }

    pub async fn load(&self) -> Result<Option<Snapshot>> {
        match self {
            Self::Memory(value) => Ok(value.lock().await.clone()),
            Self::File(path) => match tokio::fs::read(path).await {
                Ok(bytes) => Ok(Some(
                    serde_json::from_slice(&bytes).context("invalid persisted state")?,
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            },
            Self::Postgres(pool) => {
                let value: Option<serde_json::Value> = sqlx::query_scalar(
                    "SELECT snapshot FROM platform_state WHERE singleton = TRUE",
                )
                .fetch_optional(pool)
                .await?;
                value
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(Into::into)
            }
        }
    }

    pub async fn save(&self, snapshot: &Snapshot) -> Result<()> {
        match self {
            Self::Memory(value) => {
                *value.lock().await = Some(snapshot.clone());
                Ok(())
            }
            Self::File(path) => {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let temporary = path.with_extension("tmp");
                tokio::fs::write(&temporary, serde_json::to_vec(snapshot)?).await?;
                tokio::fs::rename(&temporary, path).await?;
                Ok(())
            }
            Self::Postgres(pool) => {
                let mut transaction = pool.begin().await?;
                // Serialize writes; one application instance owns the snapshot.
                sqlx::query("SELECT pg_advisory_xact_lock(1377001)")
                    .execute(&mut *transaction)
                    .await?;
                for run in snapshot.runs.values() {
                    sqlx::query(
                        "INSERT INTO runs (id, mode, epoch, state) VALUES ($1,$2,$3,$4)
                        ON CONFLICT (id) DO UPDATE SET epoch=EXCLUDED.epoch, state=EXCLUDED.state",
                    )
                    .bind(run.id)
                    .bind(
                        serde_json::to_value(run.spec.mode)?
                            .as_str()
                            .unwrap_or("unknown"),
                    )
                    .bind(i64::try_from(run.epoch)?)
                    .bind(serde_json::to_value(run)?)
                    .execute(&mut *transaction)
                    .await?;
                    for evidence in &run.evidence {
                        let (lon, lat) = evidence
                            .point
                            .map_or((None, None), |p| (Some(p.longitude), Some(p.latitude)));
                        sqlx::query("INSERT INTO evidence (run_id,id,source_id,external_id,available_at,geom,record)
                            VALUES ($1,$2,$3,$4,$5,
                            CASE WHEN $6::float8 IS NULL THEN NULL ELSE ST_SetSRID(ST_MakePoint($6,$7),4326)::geography END,$8)
                            ON CONFLICT (run_id,id) DO NOTHING")
                            .bind(run.id).bind(evidence.id).bind(&evidence.source_id).bind(&evidence.external_id)
                            .bind(evidence.available_at).bind(lon).bind(lat).bind(serde_json::to_value(evidence)?)
                            .execute(&mut *transaction).await?;
                    }
                    sqlx::query("DELETE FROM incidents WHERE run_id=$1")
                        .bind(run.id)
                        .execute(&mut *transaction)
                        .await?;
                    for incident in &run.incidents {
                        sqlx::query(
                            "INSERT INTO incidents (run_id,id,geom,record)
                            VALUES ($1,$2,ST_SetSRID(ST_MakePoint($3,$4),4326)::geography,$5)",
                        )
                        .bind(run.id)
                        .bind(incident.id)
                        .bind(incident.point.longitude)
                        .bind(incident.point.latitude)
                        .bind(serde_json::to_value(incident)?)
                        .execute(&mut *transaction)
                        .await?;
                    }
                }
                sqlx::query(
                    "INSERT INTO platform_state (singleton,snapshot) VALUES (TRUE,$1)
                    ON CONFLICT (singleton) DO UPDATE SET snapshot=EXCLUDED.snapshot",
                )
                .bind(serde_json::to_value(snapshot)?)
                .execute(&mut *transaction)
                .await?;
                transaction.commit().await?;
                Ok(())
            }
        }
    }
}
