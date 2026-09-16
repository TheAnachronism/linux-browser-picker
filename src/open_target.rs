use std::ffi::OsStr;

pub const MAX_SERIALIZED_BYTES: usize = 64 * 1024;

pub enum Error {
    InvalidUtf8,
    TooLarge,
    Malformed,
    UnsupportedScheme,
}

#[derive(Clone, Debug)]
pub struct WebTarget {
    original: String,
    matching_url: String,
    scheme: String,
    ascii_host: String,
    unicode_host: String,
    port: Option<u16>,
    path: String,
    query: Option<String>,
}

impl WebTarget {
    pub fn parse(argument: &OsStr) -> Result<Self, Error> {
        let original = argument.to_str().ok_or(Error::InvalidUtf8)?;
        if original.len() > MAX_SERIALIZED_BYTES {
            return Err(Error::TooLarge);
        }

        let parsed = url::Url::parse(original).map_err(|_| Error::Malformed)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Error::UnsupportedScheme);
        }
        let host_str = parsed.host_str().ok_or(Error::Malformed)?.to_lowercase();
        let ascii_host = match parsed.host() {
            Some(url::Host::Ipv6(_)) => host_str
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned(),
            _ => host_str,
        };
        let (unicode_host, _) = idna::domain_to_unicode(&ascii_host);
        // Keep path, query, fragment, and userinfo from the original spelling. url::Url
        // inserts "/" for an empty path and is not the Matching URL source of truth.
        let scheme_end = original.find(':').ok_or(Error::Malformed)?;
        let authority_start = scheme_end.checked_add(3).ok_or(Error::Malformed)?;
        if original.get(scheme_end..authority_start) != Some("://") {
            return Err(Error::Malformed);
        }
        let authority_tail = &original[authority_start..];
        let authority_len = authority_tail
            .find(['/', '?', '#'])
            .unwrap_or(authority_tail.len());
        let authority = &authority_tail[..authority_len];
        let userinfo = authority
            .rfind('@')
            .map(|index| &authority[..=index])
            .unwrap_or("");
        let suffix = &authority_tail[authority_len..];
        let before_fragment = suffix.split_once('#').map_or(suffix, |(before, _)| before);
        let (path, query) = match before_fragment.split_once('?') {
            Some((path, value)) => (path.to_owned(), Some(value.to_owned())),
            None => (before_fragment.to_owned(), None),
        };
        let normalized_host = match parsed.host() {
            Some(url::Host::Ipv6(_)) => format!("[{ascii_host}]"),
            _ => ascii_host.clone(),
        };
        let normalized_port = parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        let matching_url = format!(
            "{}://{userinfo}{normalized_host}{normalized_port}{suffix}",
            parsed.scheme()
        );

        Ok(Self {
            original: original.to_owned(),
            matching_url,
            scheme: parsed.scheme().to_owned(),
            ascii_host,
            unicode_host,
            port: parsed.port(),
            path,
            query,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.original
    }

    pub fn matching_url(&self) -> &str {
        &self.matching_url
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn ascii_host(&self) -> &str {
        &self.ascii_host
    }

    pub fn unicode_host(&self) -> &str {
        &self.unicode_host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
}
