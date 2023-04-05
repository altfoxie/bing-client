use futures::FutureExt;
use simplelog::error;
use tokio::task::JoinHandle;

use crate::bing::{self};

use super::{
    conversation::{Conversation, Message, Sender},
    settings::Settings,
};

#[derive(Default)]
pub struct Application {
    ctx: Option<egui::Context>,
    settings: Settings,
    input: String,
    selected_conversation: usize,
    conversations: Vec<Conversation>,
    add_conversation_handle: Option<JoinHandle<Result<Conversation, bing::Error>>>,
}

impl Application {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        app.settings = cc.storage.map_or(Settings::default(), |s| Settings::new(s));
        app.settings.apply_on_creation(&cc.egui_ctx);
        app
    }
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.ctx = Some(ctx.clone());
        self.prepare_handles(frame);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_enabled(
                    self.add_conversation_handle.is_none()
                        && !self.settings.cookie.trim().is_empty(),
                );
                if ui.button("+").clicked() {
                    self.add_conversation();
                }

                egui::ScrollArea::horizontal().show(ui, |ui| {
                    let mut delete: Option<usize> = None;

                    for (i, conversation) in self.conversations.iter_mut().enumerate() {
                        let label = ui.selectable_label(
                            self.selected_conversation == i,
                            conversation.id().to_string(),
                        );

                        if label.secondary_clicked() {
                            delete = Some(i);
                            self.selected_conversation = if i == 0 { 0 } else { i - 1 };
                        }

                        if label.clicked() {
                            self.selected_conversation = i;
                        }
                    }

                    if let Some(i) = delete {
                        self.conversations.remove(i);
                    }
                });
            });

            ui.add_space(8.0);

            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    egui::Frame::default().show(ui, |ui| {
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Cookie:");
                                ui.text_edit_singleline(&mut self.settings.cookie);
                            });

                            ui.horizontal(|ui| {
                                ui.label("Input:");
                                ui.text_edit_singleline(&mut self.input);
                                ui.set_enabled(
                                    !self.input.trim().is_empty()
                                        && !self.conversations.is_empty()
                                        && !self.conversations[self.selected_conversation]
                                            .is_busy(),
                                );
                                if ui.button("Send").clicked() {
                                    self.conversations[self.selected_conversation]
                                        .send_user_message(ctx, self.input.clone());
                                    self.input.clear();
                                }
                            });

                            ui.separator();

                            egui::Frame::none()
                                .fill(ui.visuals().faint_bg_color)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .id_source("messages_scroll_area")
                                        .show(ui, |ui| {
                                            if self.conversations.is_empty() {
                                                ui.label("No conversations");
                                                return;
                                            }

                                            let messages = self.conversations
                                                [self.selected_conversation]
                                                .msgs()
                                                .lock()
                                                .unwrap();
                                            if messages.len() == 0 {
                                                ui.label("No messages yet");
                                                return;
                                            }

                                            for message in messages.iter().rev() {
                                                match message {
                                                    Message::Text { sender, content } => {
                                                        egui::TextEdit::multiline(
                                                            &mut format!(
                                                                "{}: {}",
                                                                match sender {
                                                                    Sender::User => "You",
                                                                    Sender::Bot => "Bot",
                                                                },
                                                                content
                                                            )
                                                            .as_str(),
                                                        )
                                                        .horizontal_align(egui::Align::Center)
                                                        .desired_rows(1)
                                                        .show(ui);
                                                    }
                                                    Message::Separator => {
                                                        ui.add_space(4.0);
                                                        ui.separator();
                                                        ui.add_space(4.0);
                                                    }
                                                }
                                            }
                                        });
                                });
                        });
                    });
                },
            )
        });
    }
}

impl Application {
    fn prepare_handles(&mut self, frame: &mut eframe::Frame) {
        if let Some(conversation) = self
            .add_conversation_handle
            .as_mut()
            .and_then(|h| h.now_or_never())
            .and_then(|r| r.ok())
        {
            match conversation {
                Ok(conversation) => {
                    self.conversations.push(conversation);
                    self.selected_conversation = self.conversations.len() - 1;

                    if let Some(storage) = frame.storage_mut() {
                        self.settings.save(storage)
                    }
                }
                Err(e) => {
                    error!("failed to add conversation: {}", e);
                }
            }
            self.add_conversation_handle = None;
        }
    }

    fn add_conversation(&mut self) {
        let cookie = self.settings.cookie.clone();
        self.add_conversation_handle = Some(tokio::spawn(async {
            let conversation = bing::Conversation::new(cookie).await?;
            Ok(Conversation::new(conversation))
        }));
    }
}
