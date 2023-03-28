use std::sync::Arc;

use async_tungstenite::tungstenite::{self, Message};
use futures::{Sink, SinkExt, StreamExt};

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
use simplelog::{error, trace, warn};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use super::Error;

// Multiple objects in a single WS message are delimited by this character
const WS_DELIMITER: u8 = 0x1e;

type WsWriter = Box<dyn Sink<Message, Error = tungstenite::Error> + Unpin + Send>;

/// A conversation with the Bing chatbot
pub struct Conversation {
    id: String,
    client_id: String,
    signature: String,
    is_start_of_session: bool,
    writer: Arc<Mutex<Option<WsWriter>>>,
}

/// The result of creating a conversation
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationResult {
    conversation_id: String,
    client_id: String,
    conversation_signature: String,
}

/// An event that occurs during a conversation
#[derive(Debug)]
pub enum ConversationEvent {
    Update(String),
    Complete,
}

impl Conversation {
    /// Create a new conversation
    pub async fn new<C: Into<String>>(cookie: C) -> Result<Self, Error> {
        let mut cookie: String = cookie.into();

        // Find the _U cookie if there are multiple
        if cookie.contains(';') {
            match cookie
                .split(';')
                .filter_map(|pair| pair.trim().split_once('='))
                .map(|(key, value)| (key.to_lowercase(), value))
                .find(|(key, _)| key == "_u")
                .map(|(_, value)| value)
            {
                Some(x) => cookie = x.to_string(),
                None => return Err(Error::CookieNotFound),
            }
        }
        cookie = format!("_U={}", cookie);

        let response: ConversationResult = reqwest::Client::default()
            .request(
                Method::GET,
                "https://www.bing.com/turing/conversation/create",
            )
            .header("cookie", &cookie)
            // Hacky trick to bypass some errors
            // Thanks to Reddit :D
            .header("x-forwarded-for", "1.1.1.1")
            .send()
            .await?
            .json()
            .await?;
        trace!(
            "conversation created: <green>{}</>",
            response.conversation_id
        );

        Ok(Self {
            id: response.conversation_id,
            client_id: response.client_id,
            signature: response.conversation_signature,
            is_start_of_session: true,
            writer: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the conversation ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the client ID
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Get the conversation signature
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns true if the next message will be the start of a new session
    pub fn is_start_of_session(&self) -> bool {
        self.is_start_of_session
    }

    /// Returns true if there's an active websocket connection
    pub async fn is_busy(&self) -> bool {
        self.writer.lock().await.is_some()
    }

    /// Send a message to the chatbot
    /// Returns a `Receiver` that will receive conversation events
    pub async fn send_message<T: Into<String>>(
        &mut self,
        text: T,
    ) -> Result<mpsc::UnboundedReceiver<ConversationEvent>, Error> {
        // If the writer is some, then websocket is already open and waiting for a message
        if self.writer.lock().await.is_some() {
            return Err(Error::WsBusy);
        }

        let (stream, _) =
            async_tungstenite::tokio::connect_async("wss://sydney.bing.com/sydney/ChatHub").await?;
        let (write, mut read) = stream.split();
        *self.writer.lock().await = Some(Box::new(write));

        // Init message
        self.write(json!({
            "protocol": "json",
            "version": 1,
        }))
        .await?;

        // Idk what this is
        read.next().await.ok_or(Error::Init)?.ok();
        self.write(json!({
            "type": 6
        }))
        .await?;

        // Send the actual message
        self.write(json!({
            "arguments": [
                {
                    "source": "cib",
                    // TODO: configure these by user settings
                    "optionsSets": [
                        "nlu_direct_response_filter",
                        "deepleo",
                        "disable_emoji_spoken_text",
                        "responsible_ai_policy_235",
                        "enablemm",
                        "galileo",
                        "newspoleansgnd",
                        "cachewriteext",
                        "e2ecachewrite",
                        "dl_edge_prompt",
                        "dv3sugg"
                    ],
                    "isStartOfSession": self.is_start_of_session,
                    "message": {
                        "author": "user",
                        "inputMethod": "Keyboard",
                        "text": text.into(),
                        "messageType": "Chat"
                    },
                    "conversationSignature": self.signature,
                    "participant": {
                        "id": self.client_id
                    },
                    "conversationId": self.id,
                }
            ],
            "invocationId": Uuid::new_v4().to_string(),
            "target": "chat",
            "type": 4
        }))
        .await?;
        if self.is_start_of_session {
            self.is_start_of_session = false;
        }

        let (tx, rx) = mpsc::unbounded_channel();

        let writer_clone = self.writer.clone();
        tokio::spawn(async move {
            let writer_clone_inner = writer_clone.clone();
            read.for_each(move |message| {
                let writer_clone = writer_clone_inner.clone();
                let tx = tx.clone();
                async move {
                    if let Ok(message) = message {
                        if message.is_close() {
                            trace!("ws closed");
                            return;
                        }

                        let message = message.into_text().unwrap();

                        for object in message
                            .split(WS_DELIMITER as char)
                            .map(|obj| obj.trim())
                            .filter(|obj| !obj.is_empty())
                        {
                            let object: serde_json::Value = match serde_json::from_str(object) {
                                Ok(object) => object,
                                Err(err) => {
                                    error!("expected json, err: <red>{}</>", err);
                                    continue;
                                }
                            };

                            let type_id = match object.get("type").and_then(|v| v.as_u64()) {
                                Some(type_id) => type_id,
                                None => {
                                    error!("expected type id");
                                    continue;
                                }
                            };

                            trace!("msg type_id = <yellow>{}</>", type_id);
                            match type_id {
                                1 => {
                                    trace!("update message");
                                    match object
                                        .get("arguments")
                                        .and_then(|v| v.get(0))
                                        .and_then(|v| v.get("messages"))
                                        .and_then(|v| v.get(0))
                                        .and_then(|v| v.get("text"))
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.trim().to_string())
                                    {
                                        Some(text) => {
                                            tx.send(ConversationEvent::Update(text)).ok();
                                        }
                                        None => warn!("no text in update message"),
                                    }
                                }
                                2 => {
                                    trace!("complete message");
                                    tx.send(ConversationEvent::Complete).ok();
                                }
                                3 => {
                                    trace!("closing ws");
                                    writer_clone.lock().await.take().unwrap().close().await.ok();
                                }
                                id => {
                                    warn!("unknown type_id = <yellow>{}</>", id);
                                }
                            }
                        }
                    }
                }
            })
            .await;
            writer_clone.lock().await.take();
        });

        trace!("ws connected");
        Ok(rx)
    }

    async fn write<V: Into<serde_json::Value>>(&mut self, value: V) -> Result<(), Error> {
        let mut value = serde_json::to_vec(&value.into())?;
        value.push(WS_DELIMITER);
        self.writer
            .lock()
            .await
            .as_mut()
            .ok_or(Error::NotConnected)?
            .send(value.into())
            .await?;
        Ok(())
    }
}
