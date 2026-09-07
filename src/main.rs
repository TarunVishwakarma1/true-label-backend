mod config;

use anyhow::Error;

fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();

    let config = config::env::Env::load()?;

    println!("config: {}", config);

    Ok(())
}
