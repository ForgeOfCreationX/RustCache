use std::io;
use anyhow::Result;
use clap::{ArgAction, Parser};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use futures::future::BoxFuture;
use futures::FutureExt;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

#[derive(Debug, Clone)]
enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<RespValue>>),
}

impl RespValue {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            RespValue::SimpleString(s) => {
                out.extend_from_slice(b"+");
                out.extend_from_slice(s.as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            RespValue::Error(s) => {
                out.extend_from_slice(b"-");
                out.extend_from_slice(s.as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            RespValue::Integer(i) => {
                out.extend_from_slice(b":");
                out.extend_from_slice(i.to_string().as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            RespValue::BulkString(opt) => match opt {
                None => out.extend_from_slice(b"$-1\r\n"),
                Some(bytes) => {
                    out.extend_from_slice(b"$");
                    out.extend_from_slice(bytes.len().to_string().as_bytes());
                    out.extend_from_slice(b"\r\n");
                    out.extend_from_slice(bytes);
                    out.extend_from_slice(b"\r\n");
                }
            },
            RespValue::Array(opt) => match opt {
                None => out.extend_from_slice(b"*-1\r\n"),
                Some(values) => {
                    out.extend_from_slice(b"*");
                    out.extend_from_slice(values.len().to_string().as_bytes());
                    out.extend_from_slice(b"\r\n");
                    for v in values {
                        v.encode(out);
                    }
                }
            },
        }
    }
}

async fn read_crlf_line<R: AsyncReadExt + Unpin>(reader: &mut BufReader<R>) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(64);
    reader.read_until(b'\n', &mut buf).await?;
    if buf.is_empty() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"));
    }
    if buf.len() < 2 || buf[buf.len() - 2] != b'\r' || buf[buf.len() - 1] != b'\n' {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line missing CRLF"));
    }
    buf.truncate(buf.len() - 2);
    Ok(buf)
}

async fn read_exact_crlf<R: AsyncReadExt + Unpin>(reader: &mut BufReader<R>, len: usize) -> io::Result<Vec<u8>> {
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).await?;
    let mut crlf = [0u8; 2];
    reader.read_exact(&mut crlf).await?;
    if crlf != [b'\r', b'\n'] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bulk string missing CRLF"));
    }
    Ok(data)
}

fn read_resp<'a, R: AsyncReadExt + Unpin + Send + 'a>(reader: &'a mut BufReader<R>) -> BoxFuture<'a, io::Result<RespValue>> {
    async move {
        let mut prefix = [0u8; 1];
        reader.read_exact(&mut prefix).await?;
        match prefix[0] {
            b'+' => {
                let line = read_crlf_line(reader).await?;
                Ok(RespValue::SimpleString(String::from_utf8_lossy(&line).to_string()))
            }
            b'-' => {
                let line = read_crlf_line(reader).await?;
                Ok(RespValue::Error(String::from_utf8_lossy(&line).to_string()))
            }
            b':' => {
                let line = read_crlf_line(reader).await?;
                let s = String::from_utf8_lossy(&line);
                let i = s.parse::<i64>().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid integer"))?;
                Ok(RespValue::Integer(i))
            }
            b'$' => {
                let line = read_crlf_line(reader).await?;
                let s = String::from_utf8_lossy(&line);
                let len: isize = s.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid bulk length"))?;
                if len < 0 {
                    Ok(RespValue::BulkString(None))
                } else {
                    let data = read_exact_crlf(reader, len as usize).await?;
                    Ok(RespValue::BulkString(Some(data)))
                }
            }
            b'*' => {
                let line = read_crlf_line(reader).await?;
                let s = String::from_utf8_lossy(&line);
                let len: isize = s.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid array length"))?;
                if len < 0 {
                    Ok(RespValue::Array(None))
                } else {
                    let mut items = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        let v = read_resp(reader).await?;
                        items.push(v);
                    }
                    Ok(RespValue::Array(Some(items)))
                }
            }
            other => Err(io::Error::new(io::ErrorKind::InvalidData, format!("invalid RESP prefix: {}", other as char))),
        }
    }
    .boxed()
}

fn build_array_from_cli(args: &[String]) -> RespValue {
    let mut items: Vec<RespValue> = Vec::with_capacity(args.len());
    for a in args {
        items.push(RespValue::BulkString(Some(a.as_bytes().to_vec())));
    }
    RespValue::Array(Some(items))
}

fn format_resp(resp: &RespValue) -> String {
    match resp {
        RespValue::SimpleString(s) => s.clone(),
        RespValue::Error(s) => format!("(error) {}", s),
        RespValue::Integer(i) => i.to_string(),
        RespValue::BulkString(None) => "(nil)".to_string(),
        RespValue::BulkString(Some(b)) => match String::from_utf8(b.clone()) {
            Ok(s) => s,
            Err(_) => format!("(binary) {} bytes", b.len()),
        },
        RespValue::Array(None) => "(nil)".to_string(),
        RespValue::Array(Some(items)) => {
            let mut out = String::new();
            for (i, it) in items.iter().enumerate() {
                out.push_str(&format!("{}) {}\n", i + 1, format_resp(it)));
            }
            out.trim_end().to_string()
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "rc")]
#[command(about = "RustCache CLI (redis-cli like)", long_about = None)]
struct Cli {
    /// Server address, e.g. 127.0.0.1:9973
    #[arg(short = 'H', long = "host", default_value = "127.0.0.1")]
    host: String,

    #[arg(short = 'p', long = "port", default_value = "9973")]
    port: u16,

    /// Command to run non-interactively, e.g.: rc PING, rc SET k v
    #[arg(action = ArgAction::Append)]
    cmd: Vec<String>,
}

fn join_host_port(host: &str, port: u16) -> String {
    format!("{}:{}", host, port)
}

async fn send_command(stream: &mut TcpStream, frame: RespValue) -> Result<RespValue> {
    let mut buf = Vec::with_capacity(128);
    frame.encode(&mut buf);
    stream.write_all(&buf).await?;
    let mut reader = BufReader::new(stream);
    let resp = read_resp(&mut reader).await?;
    Ok(resp)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let addr = join_host_port(&cli.host, cli.port);
    let mut stream = TcpStream::connect(&addr).await?;

    if cli.cmd.is_empty() {
        // Interactive REPL with line editing
        println!("Connected to {}. Type commands, Ctrl+D to quit.", addr);
        let mut editor = DefaultEditor::new()?;

        loop {
            let prompt = format!("{}> ", addr);
            match editor.readline(&prompt) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
                        break;
                    }
                    let parts: Vec<String> = shell_words::split(&line).unwrap_or_else(|_| line.split_whitespace().map(|s| s.to_string()).collect());
                    let frame = build_array_from_cli(&parts);
                    let resp = send_command(&mut stream, frame).await?;
                    println!("{}", format_resp(&resp));
                }
                Err(ReadlineError::Eof) => break,
                Err(ReadlineError::Interrupted) => break,
                Err(err) => {
                    eprintln!("readline error: {}", err);
                    break;
                }
            }
        }
        Ok(())
    } else {
        let frame = build_array_from_cli(&cli.cmd);
        let resp = send_command(&mut stream, frame).await?;
        println!("{}", format_resp(&resp));
        Ok(())
    }
}


