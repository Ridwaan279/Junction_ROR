use emberwatch_domain::platform::ScenarioDataset;
use emberwatch_platform::{Platform, Store};
use sqlx::Row;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL pointing at a disposable PostGIS database"]
async fn migrations_persist_idempotent_geospatial_evidence_and_replay() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("set TEST_DATABASE_URL for a disposable database");
    let store = Store::postgres(&url).await.unwrap();
    let platform = Platform::open(store.clone(), vec![]).await.unwrap();
    let input: ScenarioDataset = serde_json::from_str(include_str!(
        "../../../contracts/fixtures/portugal-synthetic.json"
    ))
    .unwrap();
    let run = platform.load_dataset(input.clone()).await.unwrap();
    platform
        .load_into(run.id, input.evidence, input.weather)
        .await
        .unwrap();
    let Store::Postgres(pool) = store.clone() else {
        panic!("PostgreSQL store");
    };
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_evidence WHERE run_id=$1")
        .bind(run.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
    let point = sqlx::query("SELECT ST_X(geom::geometry) AS lon, ST_Y(geom::geometry) AS lat FROM platform_incidents WHERE run_id=$1")
        .bind(run.id).fetch_one(&pool).await.unwrap();
    assert_eq!(point.get::<f64, _>("lon"), -8.366);
    assert_eq!(point.get::<f64, _>("lat"), 40.735);
    let reopened = Platform::open(store, vec![]).await.unwrap();
    assert_eq!(reopened.run(run.id).await.unwrap().incidents, run.incidents);
}
