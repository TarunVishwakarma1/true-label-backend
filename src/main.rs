mod config;

use anyhow::Error;
use redis::AsyncCommands;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();

    let config = config::env::Env::load()?;
    println!("config: {}", config);

    let postgres = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.db_url)
        .await?;

    let value: String = sqlx::query_scalar("SELECT $1::TEXT")
        .bind("hello world")
        .fetch_one(&postgres)
        .await?;

    println!("{}", value);

    let client = redis::Client::open(config.redis_url)?;
    let mut con = client.get_multiplexed_async_connection().await?;

    let _: () = con.set("my_key", "my_value").await?;

    let value: String = con.get("my_key").await?;

    println!("Retrieved: {}", value);

    Ok(())
}
