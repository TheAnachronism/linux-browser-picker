use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::i18n;

pub const MAX_SERIALIZED_BYTES: usize = 64 * 1024;

pub enum Error {
    InvalidUtf8,
    TooLarge,
    Malformed,
    UnsupportedScheme,
    RemoteFile,
    Missing,
    NotRegular,
}

impl Error {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidUtf8 => i18n::text("Open Target must be valid UTF-8"),
            Self::TooLarge => i18n::text("Open Target exceeds the 65536-byte limit"),
            Self::Malformed => i18n::text(
                "Invalid Open Target: expected an existing regular local file or an absolute HTTP or HTTPS URL",
            ),
            Self::UnsupportedScheme => {
                i18n::text("Unsupported Open Target scheme; expected HTTP, HTTPS, or a local file")
            }
            Self::RemoteFile => {
                i18n::text("Unsupported Open Target: remote file URIs are not accepted")
            }
            Self::Missing | Self::NotRegular => {
                i18n::text("Open Target must be an existing regular local file")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum OpenTarget {
    Web(WebTarget),
    File(FileTarget),
}

#[derive(Clone, Debug)]
pub struct FileTarget {
    uri: String,
    path: PathBuf,
    filename: String,
    parent_summary: String,
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

impl OpenTarget {
    pub fn parse(argument: &OsStr) -> Result<Self, Error> {
        let original = argument.to_str().ok_or(Error::InvalidUtf8)?;
        if original.len() > MAX_SERIALIZED_BYTES {
            return Err(Error::TooLarge);
        }
        match leading_scheme(original) {
            Some(scheme)
                if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") =>
            {
                WebTarget::parse(argument).map(Self::Web)
            }
            Some(scheme) if scheme.eq_ignore_ascii_case("file") => {
                let parsed = url::Url::parse(original).map_err(|_| Error::Malformed)?;
                FileTarget::from_url(&parsed).map(Self::File)
            }
            Some(scheme)
                if original
                    .get(scheme.len()..)
                    .is_some_and(|rest| rest.starts_with("://")) =>
            {
                Err(Error::UnsupportedScheme)
            }
            Some(_) | None => FileTarget::from_path(Path::new(original)).map(Self::File),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Web(target) => target.as_str(),
            Self::File(target) => target.as_str(),
        }
    }

    pub fn as_web(&self) -> Option<&WebTarget> {
        match self {
            Self::Web(target) => Some(target),
            Self::File(_) => None,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Web(target) => target.unicode_host(),
            Self::File(target) => target.filename(),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Web(target) => host_forms(target),
            Self::File(target) => i18n::text_with(
                "In {directory}",
                &[("{directory}", target.parent_summary())],
            ),
        }
    }

    pub fn reveal_text(&self) -> &str {
        match self {
            Self::Web(target) => target.as_str(),
            Self::File(target) => target.path(),
        }
    }

    pub fn reveal_label(&self) -> String {
        match self {
            Self::Web(_) => i18n::text("Reveal full URL details"),
            Self::File(_) => i18n::text("Reveal absolute path"),
        }
    }

    pub fn reveal_description(&self) -> String {
        match self {
            Self::Web(_) => i18n::text("Reveals credentials, path, query, and fragment"),
            Self::File(_) => i18n::text("Reveals the absolute local file path for copying"),
        }
    }

    pub fn revalidate(&self) -> Result<(), Error> {
        match self {
            Self::Web(_) => Ok(()),
            Self::File(target) => target.revalidate(),
        }
    }
}

impl FileTarget {
    fn from_url(parsed: &url::Url) -> Result<Self, Error> {
        match parsed.host_str() {
            None | Some("") | Some("localhost") => {}
            Some(_) => return Err(Error::RemoteFile),
        }
        let path = parsed.to_file_path().map_err(|_| Error::Malformed)?;
        Self::from_path(&path)
    }

    fn from_path(path: &Path) -> Result<Self, Error> {
        let normalized = lexical_absolute(path)?;
        validate_regular_file(&normalized)?;
        let uri = url::Url::from_file_path(&normalized)
            .map_err(|_| Error::Malformed)?
            .to_string();
        let filename = normalized
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or(Error::Malformed)?
            .to_owned();
        let parent_summary = normalized
            .parent()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .unwrap_or("/")
            .to_owned();
        Ok(Self {
            uri,
            path: normalized,
            filename,
            parent_summary,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.uri
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn parent_summary(&self) -> &str {
        &self.parent_summary
    }

    pub fn path(&self) -> &str {
        self.path.to_str().unwrap_or(&self.uri)
    }

    pub fn revalidate(&self) -> Result<(), Error> {
        validate_regular_file(&self.path)
    }
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

fn leading_scheme(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let mut end = 1;
    while end < bytes.len() {
        match bytes[end] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => end += 1,
            b':' => return Some(&input[..end]),
            _ => return None,
        }
    }
    None
}

fn host_forms(target: &WebTarget) -> String {
    if target.unicode_host() == target.ascii_host() {
        i18n::text_with("Host: {host}", &[("{host}", target.ascii_host())])
    } else {
        i18n::text_with(
            "Unicode host: {unicode}\nASCII host: {ascii}",
            &[
                ("{unicode}", target.unicode_host()),
                ("{ascii}", target.ascii_host()),
            ],
        )
    }
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, Error> {
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().map_err(|_| Error::Malformed)?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in combined.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => normalized.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

fn validate_regular_file(path: &Path) -> Result<(), Error> {
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::Missing)?;
    if metadata.file_type().is_symlink() {
        match fs::metadata(path) {
            Ok(target) if target.is_file() => Ok(()),
            Ok(_) => Err(Error::NotRegular),
            Err(_) => Err(Error::Missing),
        }
    } else if metadata.is_file() {
        Ok(())
    } else {
        Err(Error::NotRegular)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    #[test]
    fn existing_local_path_containing_scheme_separator_remains_a_path() {
        let home = TempDir::new().expect("temporary home should be created");
        fs::create_dir(home.path().join("notes:")).expect("colon directory should be writable");
        let argument = home.path().join("notes://draft.txt");
        fs::write(&argument, b"not a URL").expect("fixture file should be writable");

        let target = match OpenTarget::parse(argument.as_os_str()) {
            Ok(target) => target,
            Err(error) => panic!("{}", error.message()),
        };

        match target {
            OpenTarget::File(file) => {
                assert_eq!(
                    Path::new(file.path()),
                    home.path().join("notes:").join("draft.txt")
                );
            }
            OpenTarget::Web(_) => panic!("a local path containing :// must remain a path"),
        }
    }

    #[test]
    fn relative_filename_with_colon_is_classified_as_a_path() {
        match OpenTarget::parse(std::ffi::OsStr::new("video:1.mp4")) {
            Ok(OpenTarget::File(_)) | Err(Error::Missing | Error::NotRegular) => {}
            Ok(OpenTarget::Web(_)) => panic!("a relative filename with a colon must remain a path"),
            Err(error) => panic!("{}", error.message()),
        }
    }

    #[test]
    fn empty_authority_file_uri_is_accepted() {
        let home = TempDir::new().expect("temporary home should be created");
        let path = home.path().join("page.html");
        fs::write(&path, "<html></html>").expect("fixture file should be writable");
        let uri = format!("file:{}", path.display());

        let target = match OpenTarget::parse(std::ffi::OsStr::new(&uri)) {
            Ok(target) => target,
            Err(error) => panic!("{}", error.message()),
        };

        match target {
            OpenTarget::File(file) => {
                assert_eq!(Path::new(file.path()), path.as_path());
            }
            OpenTarget::Web(_) => panic!("file:/path must be a local file Open Target"),
        }
    }

    #[test]
    fn regular_non_html_file_is_accepted_without_reading_contents() {
        let home = TempDir::new().expect("temporary home should be created");
        let path = home.path().join("notes.bin");
        fs::write(&path, b"\x00\xffnot-html").expect("binary fixture should be writable");

        let target = match OpenTarget::parse(path.as_os_str()) {
            Ok(target) => target,
            Err(error) => panic!("{}", error.message()),
        };

        assert!(matches!(target, OpenTarget::File(_)));
    }

    #[test]
    fn remote_file_authority_is_rejected() {
        assert!(matches!(
            OpenTarget::parse(std::ffi::OsStr::new("file://example.com/tmp/page.html")),
            Err(Error::RemoteFile)
        ));
    }

    #[test]
    fn matching_url_lowercases_scheme_and_host_but_preserves_path_query_and_fragment_spelling() {
        let target = match OpenTarget::parse(std::ffi::OsStr::new(
            "HTTPS://BÜCHER.example:443/Café/Path?Q=A%2FB&q=second#Frag",
        )) {
            Ok(OpenTarget::Web(target)) => target,
            Ok(OpenTarget::File(_)) => panic!("HTTPS URL must remain a web Open Target"),
            Err(error) => panic!("{}", error.message()),
        };

        assert_eq!(
            target.matching_url(),
            "https://xn--bcher-kva.example/Café/Path?Q=A%2FB&q=second#Frag"
        );
        assert_eq!(target.path(), "/Café/Path");
        assert_eq!(target.query(), Some("Q=A%2FB&q=second"));
    }
}
