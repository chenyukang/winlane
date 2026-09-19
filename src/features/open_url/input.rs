use url::{Host, Url};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputTarget {
    Url(String),
    Search(String),
}

impl InputTarget {
    pub fn url(&self) -> &str {
        match self {
            Self::Url(url) | Self::Search(url) => url,
        }
    }
}

pub fn input_target(input: &str) -> Option<InputTarget> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if let Some(url) = web_address(input) {
        return Some(InputTarget::Url(url.into()));
    }
    let url = Url::parse_with_params("https://www.google.com/search", &[("q", input)])
        .expect("static Google search URL is valid");
    Some(InputTarget::Search(url.into()))
}

fn web_address(input: &str) -> Option<Url> {
    if input
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
        || input.contains('\\')
    {
        return None;
    }
    let explicit = input
        .get(..7)
        .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
        || input
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("https://"));
    let mut url = Url::parse(&if explicit {
        input.to_owned()
    } else {
        format!("https://{input}")
    })
    .ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    // An email address is a search, not a schemeless URL with user information.
    if !explicit && (!url.username().is_empty() || url.password().is_some()) {
        return None;
    }
    match url.host()? {
        Host::Domain(host) => {
            let host = host.trim_end_matches('.');
            if !explicit && host != "localhost" && !host.contains('.') {
                return None;
            }
            if !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            }) {
                return None;
            }
        }
        Host::Ipv4(_) if !explicit => {
            // URL parsing accepts a single integer as an IPv4 address; ordinary
            // numeric search terms must not navigate to an unexpected host.
            input
                .split([':', '/', '?', '#'])
                .next()?
                .parse::<std::net::Ipv4Addr>()
                .ok()?;
        }
        Host::Ipv4(_) | Host::Ipv6(_) => {}
    }
    if !explicit
        && matches!(
            url.host(),
            Some(Host::Domain("localhost"))
                | Some(Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
                | Some(Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
        )
    {
        url.set_scheme("http").ok()?;
    }
    Some(url)
}
