use crate::db::Database;
use crate::resp::RespValue;

fn resp_ok() -> RespValue {
    RespValue::SimpleString("OK".to_string())
}
fn resp_err(msg: &str) -> RespValue {
    RespValue::Error(format!("ERR {}", msg))
}
fn resp_pong() -> RespValue {
    RespValue::SimpleString("PONG".to_string())
}

fn lower_ascii(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push((b as char).to_ascii_lowercase());
    }
    s
}

fn bulk_to_string_lossy(arg: &RespValue) -> Option<String> {
    match arg {
        RespValue::BulkString(Some(b)) => Some(String::from_utf8_lossy(b).to_string()),
        RespValue::SimpleString(s) => Some(s.clone()),
        _ => None,
    }
}

fn bulk_to_bytes(arg: &RespValue) -> Option<Vec<u8>> {
    match arg {
        RespValue::BulkString(Some(b)) => Some(b.clone()),
        RespValue::SimpleString(s) => Some(s.as_bytes().to_vec()),
        RespValue::BulkString(None) => None,
        _ => None,
    }
}

fn parse_set_ttl(
    args: &[RespValue],
) -> Result<(String, Vec<u8>, Option<std::time::Duration>), RespValue> {
    if args.len() < 2 {
        return Err(resp_err("wrong number of arguments for 'set' command"));
    }
    let key = match bulk_to_string_lossy(&args[0]) {
        Some(s) => s,
        None => return Err(resp_err("invalid key")),
    };
    let val = match bulk_to_bytes(&args[1]) {
        Some(v) => v,
        None => return Err(resp_err("invalid value")),
    };
    if args.len() == 2 {
        return Ok((key, val, None));
    }
    if args.len() == 4 {
        let opt = bulk_to_string_lossy(&args[2]).unwrap_or_default();
        if opt.eq_ignore_ascii_case("EX") {
            let secs_s = bulk_to_string_lossy(&args[3]).unwrap_or_default();
            let secs: i64 = secs_s
                .parse()
                .map_err(|_| resp_err("value is not an integer or out of range"))?;
            if secs < 0 {
                return Ok((key, val, Some(std::time::Duration::from_secs(0))));
            }
            return Ok((key, val, Some(std::time::Duration::from_secs(secs as u64))));
        }
        return Err(resp_err("syntax error"));
    }
    Err(resp_err("syntax error"))
}

fn command_to_string(args: &[RespValue]) -> Option<String> {
    if args.is_empty() {
        return None;
    }
    match &args[0] {
        RespValue::BulkString(Some(b)) => Some(lower_ascii(b)),
        RespValue::SimpleString(s) => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}

pub fn process_command(db: &Database, frame: RespValue) -> RespValue {
    let arr = match frame {
        RespValue::Array(Some(items)) => items,
        _ => return resp_err("protocol error: expected command array"),
    };
    if arr.is_empty() {
        return resp_err("protocol error: empty command");
    }
    let cmd = match command_to_string(&arr) {
        Some(c) => c,
        None => return resp_err("invalid command name"),
    };
    let args = &arr[1..];
    match cmd.as_str() {
        "ping" => {
            if args.is_empty() {
                resp_pong()
            } else if args.len() == 1 {
                match bulk_to_bytes(&args[0]) {
                    Some(b) => RespValue::BulkString(Some(b)),
                    None => resp_err("wrong number of arguments for 'ping' command"),
                }
            } else {
                resp_err("wrong number of arguments for 'ping' command")
            }
        }
        "echo" => {
            if args.len() != 1 {
                return resp_err("wrong number of arguments for 'echo' command");
            }
            match bulk_to_bytes(&args[0]) {
                Some(b) => RespValue::BulkString(Some(b)),
                None => RespValue::BulkString(None),
            }
        }
        "set" => match parse_set_ttl(args) {
            Ok((key, val, ttl)) => {
                db.set(key, val, ttl);
                resp_ok()
            }
            Err(e) => e,
        },
        "get" => {
            if args.len() != 1 {
                return resp_err("wrong number of arguments for 'get' command");
            }
            let key = match bulk_to_string_lossy(&args[0]) {
                Some(s) => s,
                None => return resp_err("invalid key"),
            };
            match db.get(&key) {
                Some(v) => RespValue::BulkString(Some(v)),
                None => RespValue::BulkString(None),
            }
        }
        "del" => {
            if args.is_empty() {
                return resp_err("wrong number of arguments for 'del' command");
            }
            let mut keys = Vec::with_capacity(args.len());
            for a in args {
                if let Some(k) = bulk_to_string_lossy(a) {
                    keys.push(k);
                }
            }
            RespValue::Integer(db.del(&keys) as i64)
        }
        "mget" => {
            if args.is_empty() {
                return RespValue::Array(Some(Vec::new()));
            }
            let mut out: Vec<RespValue> = Vec::with_capacity(args.len());
            for a in args {
                let key = match bulk_to_string_lossy(a) { Some(s) => s, None => String::new() };
                match db.get(&key) {
                    Some(v) => out.push(RespValue::BulkString(Some(v))),
                    None => out.push(RespValue::BulkString(None)),
                }
            }
            RespValue::Array(Some(out))
        }
        "mset" => {
            if args.is_empty() || args.len() % 2 != 0 {
                return resp_err("wrong number of arguments for 'mset' command");
            }
            let mut i = 0usize;
            while i < args.len() {
                let key = match bulk_to_string_lossy(&args[i]) { Some(s) => s, None => return resp_err("invalid key") };
                let val = match bulk_to_bytes(&args[i + 1]) { Some(v) => v, None => return resp_err("invalid value") };
                db.set(key, val, None);
                i += 2;
            }
            resp_ok()
        }
        "exists" => {
            if args.is_empty() {
                return resp_err("wrong number of arguments for 'exists' command");
            }
            let mut keys = Vec::with_capacity(args.len());
            for a in args {
                if let Some(k) = bulk_to_string_lossy(a) {
                    keys.push(k);
                }
            }
            RespValue::Integer(db.exists(&keys) as i64)
        }
        "incr" => {
            if args.len() != 1 {
                return resp_err("wrong number of arguments for 'incr' command");
            }
            let key = match bulk_to_string_lossy(&args[0]) {
                Some(s) => s,
                None => return resp_err("invalid key"),
            };
            match db.incr_by(key, 1) {
                Ok(v) => RespValue::Integer(v),
                Err(m) => RespValue::Error(format!("ERR {}", m)),
            }
        }
        "decr" => {
            if args.len() != 1 {
                return resp_err("wrong number of arguments for 'decr' command");
            }
            let key = match bulk_to_string_lossy(&args[0]) {
                Some(s) => s,
                None => return resp_err("invalid key"),
            };
            match db.incr_by(key, -1) {
                Ok(v) => RespValue::Integer(v),
                Err(m) => RespValue::Error(format!("ERR {}", m)),
            }
        }
        "expire" => {
            if args.len() != 2 {
                return resp_err("wrong number of arguments for 'expire' command");
            }
            let key = match bulk_to_string_lossy(&args[0]) {
                Some(s) => s,
                None => return resp_err("invalid key"),
            };
            let secs_s = match bulk_to_string_lossy(&args[1]) {
                Some(s) => s,
                None => return resp_err("value is not an integer or out of range"),
            };
            let secs: i64 = match secs_s.parse() {
                Ok(v) => v,
                Err(_) => return resp_err("value is not an integer or out of range"),
            };
            let ok = db.expire_seconds(&key, secs);
            RespValue::Integer(if ok { 1 } else { 0 })
        }
        "ttl" => {
            if args.len() != 1 {
                return resp_err("wrong number of arguments for 'ttl' command");
            }
            let key = match bulk_to_string_lossy(&args[0]) {
                Some(s) => s,
                None => return resp_err("invalid key"),
            };
            RespValue::Integer(db.ttl_seconds(&key))
        }
        "persist" => {
            if args.len() != 1 { return resp_err("wrong number of arguments for 'persist' command"); }
            let key = match bulk_to_string_lossy(&args[0]) { Some(s) => s, None => return resp_err("invalid key") };
            // remove expiration; if key exists and had expiration, return 1 else 0
            let existed = db.exists(&[key.clone()]) > 0;
            if existed {
                db.set(key.clone(), db.get(&key).unwrap_or_default(), None);
                RespValue::Integer(1)
            } else {
                RespValue::Integer(0)
            }
        }
        "flushdb" => {
            db.flushdb();
            resp_ok()
        }
        _ => RespValue::Error(format!("ERR unknown command '{}'", cmd)),
    }
}
