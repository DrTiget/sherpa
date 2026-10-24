use crate::error::{CoreError, CoreResult};
use serde_json::{json, Value};
pub fn sites(text: &str, engine: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    if engine == "nginx" {
        if !text.contains("# configuration file ") {
            return Err(CoreError::new(
                "remote.bad_output",
                "Invalid nginx configuration output",
            ));
        }
        for section in text.split("# configuration file ").skip(1) {
            let (path, config) = section
                .split_once('\n')
                .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid nginx file section"))?;
            let tokens = nginx_tokens(config)?;
            let mut depth = 0usize;
            let mut server_depth = None;
            let mut directive = Vec::new();
            let mut site = json!({});
            for token in tokens {
                match token.as_str() {
                    "{" => {
                        depth += 1;
                        if directive.first().is_some_and(|s: &String| s == "server") && directive.len() == 1 {
                            server_depth = Some(depth);
                            site = json!({"path":path.trim_end_matches(':'),"names":[],"listen":[],"root":null,"ssl":false});
                        }
                        directive.clear();
                    }
                    "}" => {
                        if server_depth == Some(depth) {
                            items.push(site.clone());
                            server_depth = None;
                        }
                        depth = depth.saturating_sub(1);
                        directive.clear();
                    }
                    ";" => {
                        if server_depth == Some(depth) {
                            if let Some(key) = directive.first() {
                                let args = &directive[1..];
                                match key.as_str() {
                                    "server_name" => site["names"] = json!(args),
                                    "root" => site["root"] = json!(args.first()),
                                    "listen" => {
                                        if args.iter().any(|a| a == "ssl") {
                                            site["ssl"] = json!(true);
                                        }
                                        if let Some(list) = site["listen"].as_array_mut() {
                                            list.push(json!(args.join(" ")));
                                        }
                                    }
                                    "ssl_certificate" => site["ssl"] = json!(true),
                                    _ => {}
                                }
                            }
                        }
                        directive.clear();
                    }
                    _ => directive.push(token),
                }
            }
        }
    } else {
        if !text.contains("VirtualHost configuration:") {
            return Err(CoreError::new(
                "remote.bad_output",
                "Invalid Apache virtual host output",
            ));
        }
        for line in text.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            if let Some(i) = parts.iter().position(|v| *v == "namevhost") {
                if let Some(name) = parts.get(i + 1) {
                    items.push(json!({"names":[name],"source":line.trim()}));
                }
            }
        }
    }
    Ok(json!({"items":items}))
}
pub fn certificates(text: &str) -> CoreResult<Value> {
    let mut items = Vec::new();
    let mut current = serde_json::Map::new();
    for line in text.lines() {
        let l = line.trim();
        if let Some(name) = l.strip_prefix("Certificate Name: ") {
            if !current.is_empty() {
                items.push(Value::Object(std::mem::take(&mut current)));
            }
            current.insert("name".into(), json!(name));
        } else if let Some(v) = l.strip_prefix("Domains: ") {
            current.insert("domains".into(), json!(v.split_whitespace().collect::<Vec<_>>()));
        } else if let Some(v) = l.strip_prefix("Expiry Date: ") {
            current.insert(
                "expires_at".into(),
                json!(v.split(" (").next().unwrap_or(v).replacen(' ', "T", 1)),
            );
        } else if let Some(v) = l.strip_prefix("Certificate Path: ") {
            current.insert("path".into(), json!(v));
        }
    }
    if !current.is_empty() {
        items.push(Value::Object(current));
    }
    if items.is_empty() && !text.contains("No certificates found") {
        return Err(CoreError::new(
            "remote.bad_output",
            "Invalid certbot certificates output",
        ));
    }
    Ok(json!({"items":items}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nginx() {
        let s="# configuration file /etc/nginx/sites-enabled/default:\nserver {\n    server_name example.org www.example.org;\n}\n";
        assert_eq!(sites(s, "nginx").unwrap()["items"][0]["names"][0], "example.org");
        assert!(sites("junk", "nginx").is_err());
    }
    #[test]
    fn apache() {
        assert_eq!(sites("VirtualHost configuration:\n         port 80 namevhost example.org (/etc/apache2/sites-enabled/site.conf:1)","apache").unwrap()["items"][0]["names"][0],"example.org");
    }
    #[test]
    fn certbot() {
        let s="  Certificate Name: example.org\n    Domains: example.org\n    Expiry Date: 2026-12-20 12:00:00+00:00 (VALID: 57 days)\n    Certificate Path: /etc/letsencrypt/live/example.org/fullchain.pem\n";
        assert_eq!(
            certificates(s).unwrap()["items"][0]["expires_at"],
            "2026-12-20T12:00:00+00:00"
        );
        assert!(certificates("junk").is_err());
        assert_eq!(certificates("No certificates found.").unwrap()["items"], json!([]));
    }
}

/// Certbot plus certificates referenced by nginx/Apache configurations.
pub fn certificate_sources(text: &str) -> CoreResult<Value> {
    let (managed, configured) = text
        .split_once("@@SHERPA_CERTS@@\n")
        .ok_or_else(|| CoreError::new("remote.bad_output", "Missing certificate sections"))?;
    let mut data = certificates(managed)?;
    if let Some(items) = data["items"].as_array_mut() {
        for block in configured.split("@@SHERPA_CERT@@\n").filter(|b| !b.trim().is_empty()) {
            let mut path = None;
            let mut expiry = None;
            let mut subject = None;
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("path=") {
                    path = Some(v);
                } else if let Some(v) = line.strip_prefix("notAfter=") {
                    expiry = Some(v);
                } else if let Some(v) = line.strip_prefix("subject=") {
                    subject = Some(v);
                }
            }
            let (path, expiry) = path
                .zip(expiry)
                .ok_or_else(|| CoreError::new("remote.bad_output", "Invalid openssl certificate output"))?;
            if !items.iter().any(|i| i["path"] == path) {
                items.push(json!({"path":path,"expires_at":openssl_date(expiry)?,"subject":subject,"source":"config"}));
            }
        }
    }
    Ok(data)
}
#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    fn openssl() {
        let d=certificate_sources("No certificates found.\n@@SHERPA_CERTS@@\npath=/etc/ssl/site.pem\nnotAfter=Dec 20 12:00:00 2026 GMT\nsubject=CN = example.org\n@@SHERPA_CERT@@\n").unwrap();
        assert_eq!(d["items"][0]["source"], "config");
        assert!(certificate_sources("junk").is_err());
    }
}

fn nginx_tokens(text: &str) -> CoreResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut comment = false;
    for c in text.chars() {
        if comment {
            if c == '\n' {
                comment = false;
            }
            continue;
        }
        if escape {
            token.push(c);
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                token.push(c);
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        if c == '#' {
            comment = true;
            continue;
        }
        if c.is_whitespace() || matches!(c, '{' | '}' | ';') {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
            if matches!(c, '{' | '}' | ';') {
                tokens.push(c.to_string());
            }
        } else {
            token.push(c);
        }
    }
    if quote.is_some() || escape {
        return Err(CoreError::new("remote.bad_output", "Invalid nginx quoting"));
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Ok(tokens)
}

fn openssl_date(text: &str) -> CoreResult<String> {
    let format =
        time::macros::format_description!("[month repr:short] [day padding:space] [hour]:[minute]:[second] [year] GMT");
    let date = time::PrimitiveDateTime::parse(text, format)
        .map_err(|_| CoreError::new("remote.bad_output", "Invalid openssl expiry date"))?;
    date.assume_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_| CoreError::new("remote.bad_output", "Invalid certificate date"))
}
