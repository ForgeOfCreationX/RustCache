use tokio::io::{self, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;

mod resp;
mod db;
mod commands;
mod banner;

use crate::resp::read_resp;
use crate::db::{Database, start_expiry_reaper};
use crate::commands::process_command;
use crate::banner::build_banner;

async fn handle_client(stream: TcpStream, db: Database) -> io::Result<()> {
    let (reader_half, mut writer_half) = stream.into_split();
    let mut reader = BufReader::new(reader_half);
    loop {
        match read_resp(&mut reader).await {
            Ok(frame) => {
                let response = process_command(&db, frame);
                let mut buf = Vec::with_capacity(128);
                response.encode(&mut buf);
                if let Err(e) = writer_half.write_all(&buf).await {
                    eprintln!("write error: {}", e);
                    break;
                }
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::UnexpectedEof {
                    eprintln!("connection error: {}", e);
                }
                break;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = std::env::var("RUSTCACHE_ADDR").unwrap_or_else(|_| "127.0.0.1:6379".to_string());
    let listener = TcpListener::bind(&addr).await?;
    // Determine the bound address to display the correct port
    let local_addr = listener.local_addr()?;
    println!("{}", build_banner(local_addr));

    let db = Database::new();
    start_expiry_reaper(db.clone()).await;

    loop {
        tokio::select! {
            res = listener.accept() => {
                let (socket, peer) = res?;
                let db_clone = db.clone();
                println!("connection from {}", peer);
                tokio::spawn(async move {
                    if let Err(e) = handle_client(socket, db_clone).await {
                        eprintln!("client error: {}", e);
                    }
                });
            }
            _ = signal::ctrl_c() => {
                println!("Shutting down on Ctrl+C");
                break;
            }
        }
    }

    Ok(())
}
