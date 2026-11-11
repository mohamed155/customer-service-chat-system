//! Keeps new primary keys UUIDv7 (MOH-42, ADR-013 gap U1).
//!
//! Three guards:
//!
//! 1. No `Uuid::new_v4()` in non-test code unless the line (or the one above) carries
//!    `// uuid-v4-ok: <why>`. A key comes from `kernel::ids::new_id()`; v4 stays only for tokens
//!    that must not reveal a time.
//! 2. No migration after `0075` gives a column `DEFAULT gen_random_uuid()` (or `uuid_generate_v4()`).
//! 3. Against a migrated database: no column default is a random v4 any more, and the default that
//!    replaced it produces version 7 values that sort by creation time, for the runtime role.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The migration that introduced `uuidv7()`. Older migrations are not edited (SQLx checksums them);
/// every migration after it is held to the rule.
const FIRST_V7_MIGRATION: u32 = 75;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == "target" || name == "tests")
            {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn uuid_v4_is_only_used_for_tokens_that_say_so() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "scan found only {} files", files.len());

    let mut offenders = Vec::new();
    let mut allowed = 0;
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        // Code after the first `#[cfg(test)]` is test code.
        let production = text.split("\n#[cfg(test)]").next().unwrap();
        let lines: Vec<&str> = production.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || !line.contains("Uuid::new_v4") {
                continue;
            }
            let marked = line.contains("uuid-v4-ok:")
                || index
                    .checked_sub(1)
                    .is_some_and(|above| lines[above].contains("uuid-v4-ok:"));
            if marked {
                allowed += 1;
            } else {
                offenders.push(format!(
                    "{}:{}: Uuid::new_v4() for a key; use kernel::ids::new_id() (or add `// uuid-v4-ok: <why>` for a token)",
                    file.display(),
                    index + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "new keys must be UUIDv7 (ADR-013):\n{}",
        offenders.join("\n")
    );
    assert!(
        allowed >= 6,
        "the scan should see the marked token sites, saw {allowed}"
    );
}

#[test]
fn migrations_after_the_v7_default_do_not_use_random_uuid_defaults() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let random_default =
        Regex::new(r"(?is)\bdefault\s+(?:public\.)?(?:gen_random_uuid|uuid_generate_v4)\s*\(")
            .unwrap();
    let mut checked = 0;
    let mut offenders = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Some(number) = name.split('_').next().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        if number <= FIRST_V7_MIGRATION || !name.ends_with(".sql") {
            continue;
        }
        checked += 1;
        let sql = fs::read_to_string(&path).unwrap();
        if random_default.is_match(&sql) {
            offenders.push(format!(
                "{name}: DEFAULT gen_random_uuid() gives a random v4 key; generate the id in the application \
                 (kernel::ids::new_id) or use DEFAULT uuidv7()"
            ));
        }
    }
    // The rule is vacuous until a migration follows 0075; check the matcher itself instead.
    assert!(random_default.is_match("id uuid PRIMARY KEY DEFAULT gen_random_uuid()"));
    assert!(random_default.is_match("ALTER COLUMN id SET DEFAULT\n  public.gen_random_uuid ()"));
    assert!(!random_default.is_match("id uuid PRIMARY KEY DEFAULT uuidv7()"));
    assert!(!random_default.is_match("SELECT gen_random_uuid()"));
    assert!(offenders.is_empty(), "{}", offenders.join("\n"));
    let _ = checked;
}

mod database {
    use super::*;

    async fn pool() -> Option<sqlx::PgPool> {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            assert!(
                std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1"),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
            );
            eprintln!("skipping uuid_hygiene database tests: DATABASE_URL not set");
            return None;
        };
        let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
        if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
            assert!(
                std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1"),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
            );
            eprintln!("skipping uuid_hygiene database tests: DATABASE_URL is unreachable");
            return None;
        }
        Some(pool)
    }

    #[tokio::test]
    async fn no_column_defaults_to_a_random_uuid() {
        let Some(pool) = pool().await else { return };
        db::testing::migrate().await;

        let random: Vec<(String, String)> = sqlx::query_as(
            "SELECT table_name::text, column_name::text FROM information_schema.columns \
             WHERE table_schema = 'public' \
               AND (column_default ILIKE '%gen_random_uuid%' OR column_default ILIKE '%uuid_generate_v4%') \
             ORDER BY 1, 2",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            random.is_empty(),
            "columns still defaulting to a random v4: {random:?}"
        );

        let v7_defaults: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_schema = 'public' AND column_name = 'id' AND column_default ILIKE '%uuidv7()%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            v7_defaults >= 40,
            "expected the converted id columns, found {v7_defaults}"
        );
    }

    #[tokio::test]
    async fn the_default_makes_version_7_ids_that_sort_by_creation_time() {
        let Some(pool) = pool().await else { return };
        db::testing::migrate().await;

        // As the runtime role (`DATABASE_URL`): it must be allowed to run the default.
        let mut previous: Option<uuid::Uuid> = None;
        for _ in 0..5 {
            let id: uuid::Uuid = sqlx::query_scalar("SELECT uuidv7()")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(id.get_version_num(), 7, "{id}");
            assert_eq!(id.get_variant(), uuid::Variant::RFC4122, "{id}");
            if let Some(before) = previous {
                assert!(before < id, "{before} should sort before {id}");
            }
            previous = Some(id);
            tokio::time::sleep(Duration::from_millis(3)).await;
        }

        // A real insert that relies on the default.
        let slug = format!("v7-default-{}", uuid::Uuid::new_v4().simple());
        let id: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
                .bind("UUIDv7 default check")
                .bind(&slug)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            id.get_version_num(),
            7,
            "a row inserted without an id gets a v7 key: {id}"
        );
        let embedded_ms = id
            .get_timestamp()
            .expect("v7 carries a timestamp")
            .to_unix();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            embedded_ms.0.abs_diff(now) < 120,
            "the embedded timestamp is the insert time"
        );
    }
}
