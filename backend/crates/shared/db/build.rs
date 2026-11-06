// `sqlx::migrate!` embeds the migration files at compile time. Without this,
// adding a new migration doesn't trigger a rebuild and the binary silently
// runs the old set (recommended by the sqlx docs for `migrate!`).
fn main() {
    println!("cargo:rerun-if-changed=../../../migrations");
}
