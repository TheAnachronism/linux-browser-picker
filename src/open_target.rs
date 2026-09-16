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
    ascii_host: String,
    unicode_host: String,
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
        let ascii_host = parsed.host_str().ok_or(Error::Malformed)?.to_lowercase();
        let (unicode_host, _) = idna::domain_to_unicode(&ascii_host);

        Ok(Self {
            original: original.to_owned(),
            ascii_host,
            unicode_host,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.original
    }

    pub fn ascii_host(&self) -> &str {
        &self.ascii_host
    }

    pub fn unicode_host(&self) -> &str {
        &self.unicode_host
    }
}
