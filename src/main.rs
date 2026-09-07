mod config;

use anyhow::Error;
use redis::AsyncCommands;
use tokio_postgres::NoTls;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();

    let config = config::env::Env::load()?;
    println!("config: {}", config);

    let postgres = tokio_postgres::connect(config.db_url.as_str(), NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = postgres.1.await {
            eprintln!("connection error: {}", e)
        }
    });

    let rows = postgres
        .0
        .query("SELECT $1::TEXT", &[&"hello world"])
        .await?;

    let value: &str = rows[0].get(0);

    println!("{}", value);

    let client = redis::Client::open(config.redis_url)?;
    let mut con = client.get_multiplexed_async_connection().await?;

    // Async SET operation
    let _: () = con.set("my_key", "my_value").await?;

    // Async GET operation
    let value: String = con.get("my_key").await?;
    println!("Retrieved: {}", value);

    Ok(())
}
