use std::ffi::OsStr;

pub const MAX_SERIALIZED_BYTES: usize = 64 * 1024;

pub enum Error {
    InvalidUtf8,
    TooLarge,
    Malformed,
    UnsupportedScheme,
}

pub struct WebTarget {
    original: String,
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
        if parsed.host().is_none() {
            return Err(Error::Malformed);
        }

        Ok(Self {
            original: original.to_owned(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.original
    }
}
