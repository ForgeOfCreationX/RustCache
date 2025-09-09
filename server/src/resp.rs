use tokio::io::{self, AsyncReadExt, BufReader};
use tokio::io::AsyncBufReadExt;
use futures::future::BoxFuture;
use futures::FutureExt;

#[derive(Debug, Clone)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Option<Vec<RespValue>>),
}

impl RespValue {
    pub fn encode(&self, out: &mut Vec<u8>) {
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

pub fn read_resp<'a, R: AsyncReadExt + Unpin + Send + 'a>(reader: &'a mut BufReader<R>) -> BoxFuture<'a, io::Result<RespValue>> {
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
