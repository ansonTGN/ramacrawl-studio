use rama::http::{HeaderMap, Method, Version};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub fn request_fingerprint(method: &Method, version: Version, headers: &HeaderMap) -> String {
    let mut canonical = format!("{}\n{:?}\n", method.as_str(), version);
    append_headers(&mut canonical, headers);
    sha256(canonical.as_bytes())
}

pub fn response_fingerprint(status: u16, version: Version, headers: &HeaderMap) -> String {
    let mut canonical = format!("{status}\n{:?}\n", version);
    append_headers(&mut canonical, headers);
    sha256(canonical.as_bytes())
}

pub fn headers_to_map(headers: &HeaderMap) -> BTreeMap<String, String> {
    let mut output = BTreeMap::<String, String>::new();
    for (name, value) in headers {
        let value = if is_sensitive_header(name.as_str()) {
            "<redacted>"
        } else {
            value.to_str().unwrap_or("<non-utf8>")
        };
        output
            .entry(name.as_str().to_owned())
            .and_modify(|current| {
                current.push_str(", ");
                current.push_str(value);
            })
            .or_insert_with(|| value.to_owned());
    }
    output
}

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn append_headers(output: &mut String, headers: &HeaderMap) {
    let mut values = headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_ascii_lowercase(),
                value.to_str().unwrap_or("<non-utf8>").to_owned(),
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    for (name, value) in values {
        output.push_str(&name);
        output.push(':');
        output.push_str(&value);
        output.push('\n');
    }
}

fn is_sensitive_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
    )
}
