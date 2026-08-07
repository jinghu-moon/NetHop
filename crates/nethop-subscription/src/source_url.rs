use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SourceUrlError {
    #[error("subscription URL must use HTTPS")]
    NonHttps,
    #[error("subscription URL cannot contain user info")]
    UserInfo,
    #[error("subscription URL must contain a host")]
    MissingHost,
}

pub fn validate_source_url(value: &str) -> Result<Url, SourceUrlError> {
    let url = Url::parse(value).map_err(|_| SourceUrlError::MissingHost)?;
    if url.scheme() != "https" {
        return Err(SourceUrlError::NonHttps);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SourceUrlError::UserInfo);
    }
    if url.host_str().is_none() {
        return Err(SourceUrlError::MissingHost);
    }
    Ok(url)
}
