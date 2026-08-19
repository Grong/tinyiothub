//! Baseline schema equivalence test (Task 2).
//!
//! Compares a database built ONLY from `20260819000001_baseline.sql` against
//! the terminal state of the old 68-migration chain (reference DB path/URL
//! passed via `TIH_OLDCHAIN_DB`). This is a one-shot export-time verification
//! gate, not a permanent CI resident: without the env var the test skips.

/// Extract a normalized schema set from a database: (type, name, normalized sql).
async fn schema_set(db_url: &str) -> std::collections::BTreeSet<(String, String, String)> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(db_url)
        .await
        .unwrap();
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT type, name, sql FROM sqlite_master
         WHERE type IN ('table','index','trigger','view') AND name NOT LIKE '_sqlx%'
           AND name NOT LIKE 'sqlite_%' ORDER BY 1,2",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|(t, n, s)| (t, n, s.split_whitespace().collect::<Vec<_>>().join(" ")))
    .collect()
}

/// Accept either a bare path (`/tmp/x.db`) or a sqlite URL.
fn to_sqlite_url(path_or_url: &str) -> String {
    if path_or_url.starts_with("sqlite:") {
        path_or_url.to_string()
    } else {
        format!("sqlite://{}", path_or_url)
    }
}

#[tokio::test]
async fn baseline_schema_matches_old_chain() {
    // 库 B（旧链终态）路径经 env var TIH_OLDCHAIN_DB 传入；缺省时 skip——
    // 它是导出时的一次性验证，非常驻 CI。
    let Ok(b_ref) = std::env::var("TIH_OLDCHAIN_DB") else {
        return;
    };
    let b_url = to_sqlite_url(&b_ref);

    // 库 A：baseline.sql 直建
    let a_path = std::env::temp_dir().join(format!("baseline-only-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&a_path);
    let a_url = format!("sqlite://{}?mode=rwc", a_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&a_url)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260819000001_baseline.sql"))
        .execute(&pool)
        .await
        .unwrap();
    drop(pool);

    let a = schema_set(&a_url).await;
    let b = schema_set(&b_url).await;

    // 逐条 diff，定位不一致的 schema 对象
    let only_in_baseline: Vec<_> = a.difference(&b).collect();
    let only_in_oldchain: Vec<_> = b.difference(&a).collect();
    assert!(
        only_in_baseline.is_empty() && only_in_oldchain.is_empty(),
        "baseline 与旧链终态 schema 不一致:\n\
         only in baseline ({}):\n{:?}\n\
         only in old chain ({}):\n{:?}",
        only_in_baseline.len(),
        only_in_baseline,
        only_in_oldchain.len(),
        only_in_oldchain,
    );
    let _ = std::fs::remove_file(&a_path);
}
