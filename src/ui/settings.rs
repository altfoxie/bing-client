pub struct Settings {
    pub ui_scale: f32,
    pub cookie: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            cookie: String::new(),
        }
    }
}

const COOKIE_KEY: &'static str = "cookie";

impl Settings {
    pub fn new(storage: &dyn eframe::Storage) -> Self {
        let mut settings = Self::default();
        settings.cookie = storage.get_string(COOKIE_KEY).unwrap_or_default();
        settings
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(COOKIE_KEY, self.cookie.clone());
        storage.flush();
    }

    pub fn apply_on_creation(&self, cc: &egui::Context) {
        cc.set_pixels_per_point(self.ui_scale);
    }
}
