use crate::{
    config::{ExtractionRule, HtmlField},
    json_path,
    model::ExtractedRecord,
};
use anyhow::{Context, Result};
use chrono::Utc;
use scraper::{ElementRef, Html, Selector};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub fn extract_records(
    crawl_id: &str,
    source_url: &Url,
    content_type: Option<&str>,
    body: &[u8],
    rules: &[ExtractionRule],
) -> Result<Vec<ExtractedRecord>> {
    let mut output = Vec::new();
    for rule in rules {
        let result = match rule {
            ExtractionRule::HtmlRecords {
                name,
                item_selector,
                fields,
            } if looks_like_html(content_type, body) => {
                extract_html(crawl_id, source_url, name, item_selector, fields, body)
            }
            ExtractionRule::JsonPath { name, path, fields }
                if looks_like_json(content_type, body) =>
            {
                extract_json(crawl_id, source_url, name, path, fields, body)
            }
            ExtractionRule::NdjsonPath { name, path, fields }
                if looks_like_ndjson(content_type) =>
            {
                extract_ndjson(crawl_id, source_url, name, path, fields, body)
            }
            _ => continue,
        };
        let mut records = result.with_context(|| format!("extraction rule failed: {rule:?}"))?;
        output.append(&mut records);
    }
    Ok(output)
}

pub fn discover_links(base: &Url, body: &[u8], selector: &str) -> Result<Vec<Url>> {
    let html = String::from_utf8_lossy(body);
    let document = Html::parse_document(&html);
    let selector = Selector::parse(selector).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let mut output = Vec::new();
    for element in document.select(&selector) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let href = href.trim();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
            || href.starts_with("javascript:")
            || href.starts_with("data:")
        {
            continue;
        }
        if let Ok(url) = base.join(href) {
            output.push(normalize_url(url));
        }
    }
    output.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    output.dedup();
    Ok(output)
}

fn extract_html(
    crawl_id: &str,
    source_url: &Url,
    name: &str,
    item_selector: &str,
    fields: &BTreeMap<String, HtmlField>,
    body: &[u8],
) -> Result<Vec<ExtractedRecord>> {
    let html = String::from_utf8_lossy(body);
    let document = Html::parse_document(&html);
    let item_selector =
        Selector::parse(item_selector).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let parsed_fields = fields
        .iter()
        .map(|(field_name, field)| {
            Selector::parse(&field.selector)
                .map(|selector| (field_name, field, selector))
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut output = Vec::new();
    for (ordinal, item) in document.select(&item_selector).enumerate() {
        let mut data = Map::new();
        for (field_name, field, selector) in &parsed_fields {
            let values = item
                .select(selector)
                .map(|element| extract_html_value(element, field))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            let value = if field.all {
                Value::Array(values.into_iter().map(Value::String).collect())
            } else {
                values.into_iter().next().map_or(Value::Null, Value::String)
            };
            data.insert((*field_name).clone(), value);
        }
        data.insert(
            "_source_url".to_owned(),
            Value::String(source_url.to_string()),
        );
        output.push(ExtractedRecord {
            crawl_id: crawl_id.to_owned(),
            source_url: source_url.to_string(),
            rule: name.to_owned(),
            ordinal,
            extracted_at: Utc::now(),
            data,
        });
    }
    Ok(output)
}

fn extract_html_value(element: ElementRef<'_>, field: &HtmlField) -> String {
    let mut value = if let Some(attribute) = &field.attribute {
        element
            .value()
            .attr(attribute)
            .unwrap_or_default()
            .to_owned()
    } else {
        element.text().collect::<Vec<_>>().join(" ")
    };
    if field.trim {
        value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    value
}

fn extract_json(
    crawl_id: &str,
    source_url: &Url,
    name: &str,
    path: &str,
    fields: &BTreeMap<String, String>,
    body: &[u8],
) -> Result<Vec<ExtractedRecord>> {
    let root: Value = serde_json::from_slice(body).context("invalid JSON body")?;
    records_from_json(crawl_id, source_url, name, path, fields, &root)
}

fn extract_ndjson(
    crawl_id: &str,
    source_url: &Url,
    name: &str,
    path: &str,
    fields: &BTreeMap<String, String>,
    body: &[u8],
) -> Result<Vec<ExtractedRecord>> {
    let text = String::from_utf8_lossy(body);
    let mut output = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let root: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid NDJSON at line {}", line_number + 1))?;
        output.extend(records_from_json(
            crawl_id, source_url, name, path, fields, &root,
        )?);
    }
    for (ordinal, record) in output.iter_mut().enumerate() {
        record.ordinal = ordinal;
    }
    Ok(output)
}

fn records_from_json(
    crawl_id: &str,
    source_url: &Url,
    name: &str,
    path: &str,
    fields: &BTreeMap<String, String>,
    root: &Value,
) -> Result<Vec<ExtractedRecord>> {
    let selected = json_path::select(root, path)?;
    let mut output = Vec::new();
    for (ordinal, value) in selected.into_iter().enumerate() {
        let mut data = Map::new();
        if fields.is_empty() {
            match value {
                Value::Object(values) => data.extend(values.clone()),
                other => {
                    data.insert("value".to_owned(), other.clone());
                }
            }
        } else {
            for (field, field_path) in fields {
                let selected_field = json_path::select(value, field_path)?;
                let field_value = match selected_field.as_slice() {
                    [] => Value::Null,
                    [single] => (*single).clone(),
                    many => Value::Array(many.iter().map(|value| (*value).clone()).collect()),
                };
                data.insert(field.clone(), field_value);
            }
        }
        data.insert(
            "_source_url".to_owned(),
            Value::String(source_url.to_string()),
        );
        output.push(ExtractedRecord {
            crawl_id: crawl_id.to_owned(),
            source_url: source_url.to_string(),
            rule: name.to_owned(),
            ordinal,
            extracted_at: Utc::now(),
            data,
        });
    }
    Ok(output)
}

fn normalize_url(mut url: Url) -> Url {
    url.set_fragment(None);
    if matches!(
        (url.scheme(), url.port()),
        ("http", Some(80)) | ("https", Some(443))
    ) {
        let _ = url.set_port(None);
    }
    url
}

fn looks_like_html(content_type: Option<&str>, body: &[u8]) -> bool {
    content_type
        .is_some_and(|value| value.contains("text/html") || value.contains("application/xhtml"))
        || body.starts_with(b"<!DOCTYPE html")
        || body.starts_with(b"<html")
}

fn looks_like_json(content_type: Option<&str>, body: &[u8]) -> bool {
    content_type.is_some_and(|value| value.contains("application/json") || value.contains("+json"))
        || matches!(
            body.iter().find(|byte| !byte.is_ascii_whitespace()),
            Some(b'{') | Some(b'[')
        )
}

fn looks_like_ndjson(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| {
        value.contains("application/x-ndjson") || value.contains("application/ndjson")
    })
}
