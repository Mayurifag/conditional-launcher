use crate::config::AppConfig;
use crate::os::OsOperations;
use eframe::egui;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Default)]
pub(super) struct Icons(HashMap<String, egui::TextureHandle>);

impl Icons {
    pub fn draw(&mut self, ui: &mut egui::Ui, app: &AppConfig, os_ops: &dyn OsOperations) {
        let texture = app
            .icon
            .as_deref()
            .filter(|name| !name.is_empty())
            .and_then(|name| match self.0.entry(name.to_owned()) {
                Entry::Occupied(entry) => Some(entry.into_mut()),
                Entry::Vacant(entry) => {
                    let (rgba, size) = os_ops.get_app_icon_rgba(app)?;
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                    Some(entry.insert(ui.ctx().load_texture(name, image, Default::default())))
                }
            });
        if let Some(texture) = texture {
            ui.add(egui::Image::new(&*texture).max_size(egui::vec2(20.0, 20.0)));
        } else {
            ui.add_sized(
                [20.0, 20.0],
                egui::Label::new(app.name.chars().next().unwrap_or('?').to_string()),
            );
        }
    }
}
