//! Optional PostgreSQL storage. The demo is explicit about in-memory fallback.
use crate::Engine;
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}
impl PgStore {
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        sqlx::migrate!("../../migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn load(&self, run_id: &str) -> Result<Option<Engine>, sqlx::Error> {
        let record: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT state FROM runs WHERE id = $1")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await?;
        record
            .map(|(value,)| {
                serde_json::from_value(value).map_err(|e| sqlx::Error::Decode(Box::new(e)))
            })
            .transpose()
    }

    pub async fn save(&self, engine: &Engine) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let mode = match engine.scenario.mode {
            crate::DataMode::Live => "live",
            crate::DataMode::Historical => "historical",
            crate::DataMode::Synthetic => "synthetic",
        };
        let state = serde_json::to_value(engine).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        sqlx::query("INSERT INTO runs(id, mode, epoch, state) VALUES ($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET epoch = EXCLUDED.epoch, state = EXCLUDED.state, updated_at = now()")
            .bind(&engine.scenario.run_id).bind(mode).bind(engine.scenario.epoch as i64).bind(state).execute(&mut *tx).await?;
        for evidence in engine.evidence.values() {
            let (lon, lat) = evidence
                .position
                .map_or((None, None), |[x, y]| (Some(x), Some(y)));
            sqlx::query("INSERT INTO evidence(run_id,id,source_id,origin_group,event_at,captured_at,available_at,position,record) VALUES ($1,$2,$3,$4,$5,$6,$7,CASE WHEN $8::float8 IS NULL THEN NULL ELSE ST_SetSRID(ST_MakePoint($8,$9),4326) END,$10) ON CONFLICT(run_id,id) DO NOTHING")
                .bind(&evidence.run_id).bind(&evidence.id).bind(&evidence.provenance.source_id).bind(&evidence.provenance.origin_group)
                .bind(evidence.provenance.event_at).bind(evidence.provenance.captured_at).bind(evidence.provenance.available_at).bind(lon).bind(lat)
                .bind(serde_json::to_value(evidence).map_err(|e| sqlx::Error::Encode(Box::new(e)))?).execute(&mut *tx).await?;
        }
        tx.commit().await
    }

    /// Durable verified receipt only. Normalization/ingestion is a separate step.
    pub async fn persist_receipt(
        &self,
        run_id: &str,
        source_id: &str,
        id: &str,
        body_hash: &str,
        record: serde_json::Value,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query("INSERT INTO webhook_receipts(run_id,source_id,id,body_hash,record) VALUES($1,$2,$3,$4,$5) ON CONFLICT(source_id,id) DO NOTHING")
            .bind(run_id).bind(source_id).bind(id).bind(body_hash).bind(record).execute(&mut *tx).await?.rows_affected() == 1;
        if !inserted {
            let (original_hash,): (String,) = sqlx::query_as(
                "SELECT body_hash FROM webhook_receipts WHERE source_id=$1 AND id=$2",
            )
            .bind(source_id)
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            if original_hash != body_hash {
                return Err(sqlx::Error::Protocol(
                    "receipt identity reused with different content".into(),
                ));
            }
        }
        tx.commit().await?;
        Ok(inserted)
    }
}
