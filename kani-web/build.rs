fn main() {
    // Re-run if any migration changes (sqlx compile-time checks pick up DATABASE_URL from .env)
    println!("cargo:rerun-if-changed=../migrations");
}
