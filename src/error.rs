pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Network error: {0}")]
    Surf(String),

    #[error("Error formatting content: {0}")]
    Fmt(#[from] std::fmt::Error),
}
