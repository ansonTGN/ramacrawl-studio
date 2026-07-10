use crate::config::UserAgentProfile;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct RequestProfile {
    pub name: String,
    pub headers: BTreeMap<String, String>,
}

impl RequestProfile {
    pub fn from_config(profile: &UserAgentProfile) -> Self {
        match profile {
            UserAgentProfile::ChromeLinux => Self::chrome_linux(),
            UserAgentProfile::FirefoxLinux => Self::firefox_linux(),
            UserAgentProfile::SafariMacos => Self::safari_macos(),
            UserAgentProfile::MinimalBot => Self::minimal_bot(),
            UserAgentProfile::Custom {
                user_agent,
                headers,
            } => {
                let mut output = headers.clone();
                output.insert("user-agent".to_owned(), user_agent.clone());
                Self {
                    name: "custom".to_owned(),
                    headers: output,
                }
            }
        }
    }

    fn chrome_linux() -> Self {
        let headers = BTreeMap::from([
            ("user-agent".to_owned(), "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36".to_owned()),
            ("accept".to_owned(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".to_owned()),
            ("accept-language".to_owned(), "es-ES,es;q=0.9,en;q=0.7".to_owned()),
            ("sec-ch-ua".to_owned(), "\"Chromium\";v=\"138\", \"Not=A?Brand\";v=\"24\"".to_owned()),
            ("sec-ch-ua-mobile".to_owned(), "?0".to_owned()),
            ("sec-ch-ua-platform".to_owned(), "\"Linux\"".to_owned()),
            ("upgrade-insecure-requests".to_owned(), "1".to_owned()),
        ]);
        Self {
            name: "chrome_linux".to_owned(),
            headers,
        }
    }

    fn firefox_linux() -> Self {
        let headers = BTreeMap::from([
            ("user-agent".to_owned(), "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0".to_owned()),
            ("accept".to_owned(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".to_owned()),
            ("accept-language".to_owned(), "es-ES,es;q=0.9,en;q=0.6".to_owned()),
            ("upgrade-insecure-requests".to_owned(), "1".to_owned()),
        ]);
        Self {
            name: "firefox_linux".to_owned(),
            headers,
        }
    }

    fn safari_macos() -> Self {
        let headers = BTreeMap::from([
            ("user-agent".to_owned(), "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15".to_owned()),
            ("accept".to_owned(), "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".to_owned()),
            ("accept-language".to_owned(), "es-ES,es;q=0.9,en;q=0.6".to_owned()),
        ]);
        Self {
            name: "safari_macos".to_owned(),
            headers,
        }
    }

    fn minimal_bot() -> Self {
        let headers = BTreeMap::from([
            (
                "user-agent".to_owned(),
                "RamaControlledExtractor/0.1 (+https://example.invalid/bot-info)".to_owned(),
            ),
            (
                "accept".to_owned(),
                "text/html,application/json,application/x-ndjson;q=0.9,*/*;q=0.5".to_owned(),
            ),
            (
                "accept-language".to_owned(),
                "es-ES,es;q=0.9,en;q=0.6".to_owned(),
            ),
        ]);
        Self {
            name: "minimal_bot".to_owned(),
            headers,
        }
    }
}
