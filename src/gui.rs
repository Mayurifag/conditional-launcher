mod conditions;
mod editor;
mod icons;
mod status;

use crate::app::Launcher;
use editor::{EditAction, Editor};
use eframe::egui;
use icons::Icons;
use status::Status;
use std::time::Duration;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let app = Launcher::load()?;
    let status = Status::new()?;
    const HEIGHT_PER_APP: f32 = 95.0;
    const PADDING: f32 = 30.0;
    const MIN_HEIGHT: f32 = 150.0;
    const MAX_HEIGHT: f32 = 700.0;
    let height = if app.apps.is_empty() {
        MIN_HEIGHT
    } else {
        (app.apps.len().min(8) as f32 * HEIGHT_PER_APP + PADDING).min(MAX_HEIGHT)
    };
    let options = eframe::NativeOptions {
        // Keep the existing OpenGL backend; eframe now defaults to wgpu.
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, height])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "Conditional Launcher",
        options,
        Box::new(|cc| Ok(Box::new(GuiApp::new(cc, app, status)))),
    )
    .map_err(Into::into)
}

struct GuiApp {
    app: Launcher,
    icons: Icons,
    status: Status,
    editor: Option<Editor>,
    error: Option<String>,
}

impl GuiApp {
    fn new(cc: &eframe::CreationContext<'_>, app: Launcher, status: Status) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self {
            app,
            icons: Icons::default(),
            status,
            editor: None,
            error: None,
        }
    }

    fn draw_apps(&mut self, ui: &mut egui::Ui) {
        let mut needs_save = false;
        let mut management_changes = Vec::new();
        if self.app.apps.is_empty() {
            ui.label("No autostart applications found.");
        }
        for (index, app) in self.app.apps.iter_mut().enumerate() {
            if index > 0 {
                ui.add_space(8.0);
            }
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.icons.draw(ui, app, self.app.os_ops.as_ref());
                    ui.label(egui::RichText::new(&app.name).strong());
                });

                if let Some(editor) = self
                    .editor
                    .as_mut()
                    .filter(|editor| editor.app_name == app.name)
                {
                    match editor.draw(ui) {
                        EditAction::Save => {
                            if let Some(editor) = self.editor.take() {
                                editor.apply(app);
                                needs_save = true;
                            }
                        }
                        EditAction::Cancel => self.editor = None,
                        EditAction::Continue => {}
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&app.command).small().monospace());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let running =
                                self.status.running.get(&app.name).copied().unwrap_or(false);
                            if !running
                                && ui
                                    .button("Run")
                                    .on_hover_text("Launch this application now.")
                                    .clicked()
                                && let Err(error) = self.app.os_ops.run_now(app)
                            {
                                self.error = Some(format!("{error:#}"));
                            }
                            if app.is_managed && ui.button("Edit").clicked() {
                                self.editor = Some(Editor::new(app));
                            }
                        });
                    });
                    if let Some(dir) = &app.working_dir
                        && !dir.as_os_str().is_empty()
                    {
                        ui.label(
                            egui::RichText::new(format!("Working Dir: {}", dir.display()))
                                .small()
                                .monospace(),
                        );
                    }
                }

                ui.separator();
                let previous = app.conditions.clone();
                conditions::draw(
                    ui,
                    app,
                    &self.app.available_partitions,
                    self.app.os_ops.as_ref(),
                    self.status.has_internet,
                    &self.status.disks,
                );
                if app.conditions != previous {
                    let conditions = std::mem::replace(&mut app.conditions, previous);
                    management_changes.push((index, conditions));
                }
            });
        }
        for (index, conditions) in management_changes {
            if let Err(error) = self.app.update_conditions(index, conditions) {
                self.error = Some(format!("{error:#}"));
            }
        }
        if needs_save {
            self.save();
        }
    }

    fn save(&mut self) {
        self.error = self.app.save().err().map(|error| format!("{error:#}"));
    }
}

impl eframe::App for GuiApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(1));
        if let Err(error) = self.status.refresh(&mut self.app) {
            self.error = Some(format!("{error:#}"));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame {
                inner_margin: egui::Margin::symmetric(10, 10),
                ..Default::default()
            })
            .show(ui, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                    ui.horizontal(|ui| {
                        if ui.button("Save configuration").clicked() {
                            self.save();
                        }
                        if ui.button("Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                    ui.separator();
                }
                egui::ScrollArea::vertical().show(ui, |ui| self.draw_apps(ui));
            });
    }
}
