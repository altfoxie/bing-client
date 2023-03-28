use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;

use crate::bing::{self, ConversationEvent};

// A wrapped conversation, which stores the conversation's messages history.
pub struct Conversation {
    /// The conversation's id.
    id: String,
    /// The wrapped conversation.
    bing_conversation: Arc<tokio::sync::Mutex<bing::Conversation>>,
    /// Order of the messages is from the newest to the oldest.
    messages: Arc<std::sync::Mutex<Vec<Message>>>,
    /// Handle to the channel that updates the bot's answer.
    handle: Option<JoinHandle<()>>,
}

impl Conversation {
    pub fn new(bing_conversation: bing::Conversation) -> Self {
        Self {
            id: bing_conversation
                .id()
                .to_string()
                .chars()
                .rev()
                .take(8)
                .collect(),
            bing_conversation: Arc::new(tokio::sync::Mutex::new(bing_conversation)),
            messages: Arc::new(Mutex::new(vec![])),
            handle: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn msgs(&mut self) -> &Arc<Mutex<Vec<Message>>> {
        self.prepare_handle();
        &self.messages
    }

    pub fn is_busy(&self) -> bool {
        self.handle.is_some()
    }

    pub fn send_user_message<C: Into<String>>(&mut self, ctx: &egui::Context, content: C) {
        let content = content.into();

        self.messages.lock().unwrap().push(Message::Text {
            sender: Sender::User,
            content: content.clone(),
        });

        let messages = self.messages.clone();
        let bing_conversation = self.bing_conversation.clone();
        let ctx = ctx.clone();
        self.handle = Some(tokio::spawn(async move {
            let mut bing_conversation = bing_conversation.lock().await;
            let mut channel = bing_conversation.send_message(content).await.unwrap();

            let mut needs_creation = true;
            while let Some(event) = channel.recv().await {
                match event {
                    ConversationEvent::Update(string) => {
                        if needs_creation {
                            messages.lock().unwrap().push(Message::Text {
                                sender: Sender::Bot,
                                content: string,
                            });
                            needs_creation = false;
                        } else {
                            for msg in messages.lock().unwrap().iter_mut().rev() {
                                if let Message::Text { sender, content } = msg {
                                    if matches!(sender, Sender::Bot) {
                                        *content = string + "...";
                                        break;
                                    }
                                }
                            }
                        }

                        ctx.request_repaint();
                    }
                    ConversationEvent::Complete => {
                        for msg in messages.lock().unwrap().iter_mut().rev() {
                            if let Message::Text { sender, content } = msg {
                                if matches!(sender, Sender::Bot) {
                                    *content = content[..content.len() - 3].to_string();
                                    break;
                                }
                            }
                        }
                        messages.lock().unwrap().push(Message::Separator);
                        ctx.request_repaint();
                        break;
                    }
                }
            }
        }));
    }

    fn prepare_handle(&mut self) {
        if let Some(handle) = &self.handle {
            if handle.is_finished() {
                self.handle = None;
            }
        }
    }
}

#[derive(Debug)]
pub enum Message {
    Text { sender: Sender, content: String },
    Separator,
}

#[derive(Debug)]
pub enum Sender {
    User,
    Bot,
}
