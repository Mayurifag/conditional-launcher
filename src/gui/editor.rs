use crate::config::AppConfig;
use eframe::egui;
use std::path::PathBuf;

pub(super) struct Editor {
    pub app_name: String,
    command: String,
    working_dir: String,
}

pub(super) enum EditAction {
    Continue,
    Save,
    Cancel,
}

impl Editor {
    pub fn new(app: &AppConfig) -> Self {
        Self {
            app_name: app.name.clone(),
            command: app.command.clone(),
            working_dir: app
                .working_dir
                .as_deref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) -> EditAction {
        for (label, value) in [
            ("Command:", &mut self.command),
            ("Working Dir:", &mut self.working_dir),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                EditAction::Save
            } else if ui.button("Cancel").clicked() {
                EditAction::Cancel
            } else {
                EditAction::Continue
            }
        })
        .inner
    }

    pub fn apply(self, app: &mut AppConfig) {
        app.command = self.command;
        app.working_dir = if self.working_dir.is_empty() {
            None
        } else {
            Some(PathBuf::from(self.working_dir))
        };
    }
}
