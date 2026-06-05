use refinery::embed_migrations;

embed_migrations!("./migrations");

/// Run all pending migrations
pub fn run_migrations(conn: &mut rusqlite::Connection) -> Result<(), refinery::Error> {
    migrations::runner().run(conn)?;
    Ok(())
}

