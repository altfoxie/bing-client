use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("cookie \"_U\" not found")]
    CookieNotFound,

    #[error("not connected")]
    NotConnected,

    #[error("init error, failed to read first message")]
    Init,

    #[error("websocket connection is busy")]
    WsBusy,

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error(transparent)]
    Ws(#[from] async_tungstenite::tungstenite::Error),
}
