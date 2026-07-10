use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Default)]
pub struct RobotsRules {
    groups: Vec<RobotsGroup>,
}

#[derive(Debug, Clone, Default)]
struct RobotsGroup {
    agents: Vec<String>,
    rules: Vec<PathRule>,
    crawl_delay_secs: Option<f64>,
}

#[derive(Debug, Clone)]
struct PathRule {
    allow: bool,
    pattern: String,
}

impl RobotsRules {
    pub fn deny_all() -> Self {
        Self {
            groups: vec![RobotsGroup {
                agents: vec!["*".to_owned()],
                rules: vec![PathRule {
                    allow: false,
                    pattern: "/".to_owned(),
                }],
                crawl_delay_secs: None,
            }],
        }
    }

    pub fn parse(input: &str) -> Self {
        let mut groups = Vec::<RobotsGroup>::new();
        let mut current = RobotsGroup::default();
        let mut seen_rule = false;

        for raw_line in input.lines() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                if !current.agents.is_empty() {
                    groups.push(std::mem::take(&mut current));
                    seen_rule = false;
                }
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => {
                    if seen_rule && !current.agents.is_empty() {
                        groups.push(std::mem::take(&mut current));
                        seen_rule = false;
                    }
                    current.agents.push(value.to_ascii_lowercase());
                }
                "allow" | "disallow" if !current.agents.is_empty() => {
                    seen_rule = true;
                    if !value.is_empty() {
                        current.rules.push(PathRule {
                            allow: key == "allow",
                            pattern: value.to_owned(),
                        });
                    }
                }
                "crawl-delay" if !current.agents.is_empty() => {
                    seen_rule = true;
                    current.crawl_delay_secs = value.parse::<f64>().ok().filter(|v| *v >= 0.0);
                }
                _ => {}
            }
        }
        if !current.agents.is_empty() {
            groups.push(current);
        }
        Self { groups }
    }

    pub fn allowed(&self, agent: &str, url: &Url) -> bool {
        let path = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        let groups = self.matching_groups(agent);
        let mut best: Option<(usize, bool)> = None;
        for group in groups {
            for rule in &group.rules {
                if pattern_matches(&rule.pattern, &path) {
                    let length = rule.pattern.trim_end_matches('$').len();
                    match best {
                        None => best = Some((length, rule.allow)),
                        Some((best_length, best_allow))
                            if length > best_length
                                || (length == best_length && rule.allow && !best_allow) =>
                        {
                            best = Some((length, rule.allow));
                        }
                        _ => {}
                    }
                }
            }
        }
        best.is_none_or(|(_, allow)| allow)
    }

    pub fn crawl_delay_ms(&self, agent: &str) -> Option<u64> {
        self.matching_groups(agent)
            .filter_map(|group| group.crawl_delay_secs)
            .max_by(f64::total_cmp)
            .map(|seconds| (seconds * 1000.0).ceil() as u64)
    }

    fn matching_groups<'a>(&'a self, agent: &'a str) -> impl Iterator<Item = &'a RobotsGroup> {
        let agent = agent.to_ascii_lowercase();
        let specific_exists = self.groups.iter().any(|group| {
            group
                .agents
                .iter()
                .any(|candidate| candidate != "*" && agent.contains(candidate))
        });
        self.groups.iter().filter(move |group| {
            group.agents.iter().any(|candidate| {
                if specific_exists {
                    candidate != "*" && agent.contains(candidate)
                } else {
                    candidate == "*"
                }
            })
        })
    }
}

#[derive(Debug, Default)]
pub struct RobotsCache {
    entries: tokio::sync::RwLock<HashMap<String, RobotsRules>>,
}

impl RobotsCache {
    pub async fn get(&self, origin: &str) -> Option<RobotsRules> {
        self.entries.read().await.get(origin).cloned()
    }

    pub async fn insert(&self, origin: String, rules: RobotsRules) {
        self.entries.write().await.insert(origin, rules);
    }
}

fn pattern_matches(pattern: &str, path: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let raw = pattern.trim_end_matches('$');
    let mut regex = String::from("^");
    for character in raw.chars() {
        match character {
            '*' => regex.push_str(".*"),
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                regex.push('\\');
                regex.push(character);
            }
            other => regex.push(other),
        }
    }
    if anchored {
        regex.push('$');
    }
    regex::Regex::new(&regex).is_ok_and(|compiled| compiled.is_match(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_rule_wins() {
        let robots =
            RobotsRules::parse("User-agent: *\nDisallow: /private\nAllow: /private/public\n");
        let blocked = Url::parse("https://example.com/private/a").ok();
        let allowed = Url::parse("https://example.com/private/public/a").ok();
        assert!(blocked.is_some_and(|url| !robots.allowed("bot", &url)));
        assert!(allowed.is_some_and(|url| robots.allowed("bot", &url)));
    }
}
