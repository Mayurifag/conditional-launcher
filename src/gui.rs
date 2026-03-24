use crate::app::{ConditionalLauncherApp, check_app_conditions};
use crate::config::AppConfig;
use crate::os::{OsOperations, PartitionInfo};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use sysinfo::{Disks, ProcessRefreshKind, RefreshKind, System};

pub struct GuiApp {
    pub app: ConditionalLauncherApp,
    texture_cache: HashMap<String, egui::TextureHandle>,
    last_autostart_check: Option<SystemTime>,
    sys: System,
    last_cache_update: SystemTime,
    cached_internet_ok: bool,
    cached_disks: Disks,
    cached_running_status: HashMap<String, bool>,
    editing_app_name: Option<String>,
    edit_buffer_command: String,
    edit_buffer_working_dir: String,
}

impl GuiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, apps: Vec<AppConfig>) -> Self {
        let app = ConditionalLauncherApp::new(apps);

        Self {
            app,
            texture_cache: HashMap::new(),
            last_autostart_check: None,
            sys: System::new_all(),
            last_cache_update: SystemTime::UNIX_EPOCH,
            cached_internet_ok: false,
            cached_disks: Disks::new(),
            cached_running_status: HashMap::new(),
            editing_app_name: None,
            edit_buffer_command: String::new(),
            edit_buffer_working_dir: String::new(),
        }
    }

    fn refresh_autostart_list(&mut self) {
        let fresh_unmanaged = self.app.os_ops.get_autostart_apps();

        self.app
            .apps
            .retain(|app| app.is_managed || fresh_unmanaged.iter().any(|u| u.name == app.name));

        for app in fresh_unmanaged {
            if !self.app.apps.iter().any(|a| a.name == app.name) {
                self.app.apps.push(app);
            }
        }

        self.app
            .apps
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }
}

fn get_or_load_icon<'a>(
    texture_cache: &'a mut HashMap<String, egui::TextureHandle>,
    ctx: &egui::Context,
    app: &AppConfig,
    os_ops: &dyn OsOperations,
) -> Option<&'a egui::TextureHandle> {
    let icon_key = app.icon.as_deref().filter(|s| !s.is_empty())?.to_string();

    if !texture_cache.contains_key(&icon_key)
        && let Some((rgba, [w, h])) = os_ops.get_app_icon_rgba(app)
    {
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        let texture = ctx.load_texture(&icon_key, color_image, Default::default());
        texture_cache.insert(icon_key.clone(), texture);
    }

    texture_cache.get(&icon_key)
}

fn draw_condition_controls(
    ui: &mut egui::Ui,
    app: &mut AppConfig,
    available_partitions: &[PartitionInfo],
    os_ops: &dyn OsOperations,
    cached_internet_ok: bool,
    cached_disks: &Disks,
) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.conditions.internet, "Internet")
            .on_hover_text(
                "If checked, this app will only launch if there is an active internet connection.",
            );

        let status = check_app_conditions(os_ops, app, cached_internet_ok, cached_disks);

        if app.conditions.internet {
            let text = if status.internet_ok { "✅" } else { "❌" };
            ui.label(text)
                .on_hover_text("Current internet connection status.");
        }

        ui.separator();

        ui.label("Partition:").on_hover_text(
            "If a partition is selected, this app will only launch if that partition is mounted.",
        );
        let selected_text = app
            .conditions
            .partition_mounted
            .as_deref()
            .unwrap_or("None");

        egui::ComboBox::from_id_salt(&app.name)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.conditions.partition_mounted, None, "None");
                for p in available_partitions.iter() {
                    let display_label = &p.mount_point;
                    let details = if p.fs_type.is_empty() {
                        format!("({})", p.size)
                    } else {
                        format!("({}, {})", p.fs_type, p.size)
                    };
                    ui.selectable_value(
                        &mut app.conditions.partition_mounted,
                        Some(p.mount_point.clone()),
                        format!("{display_label} {details}"),
                    );
                }
            });

        if app.conditions.partition_mounted.is_some() {
            let text = if status.partition_ok { "✅" } else { "❌" };
            ui.label(text)
                .on_hover_text("Current status of the selected partition.");
        }
    });
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        ctx.request_repaint_after(std::time::Duration::from_secs(5));

        if self
            .last_cache_update
            .elapsed()
            .unwrap_or_default()
            .as_secs()
            >= 5
        {
            self.cached_internet_ok = self.app.os_ops.check_internet_connection();
            self.cached_disks.refresh(true);

            self.sys.refresh_specifics(
                RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
            );

            // Refresh autostart list every 30 seconds (cross-platform)
            let should_refresh = self
                .last_autostart_check
                .map(|t| t.elapsed().unwrap_or_default().as_secs() >= 30)
                .unwrap_or(true);
            if should_refresh {
                self.refresh_autostart_list();
                self.last_autostart_check = Some(SystemTime::now());
            }

            self.cached_running_status.clear();
            for app in self.app.apps.iter() {
                let is_running = self.app.os_ops.is_app_running(app, &self.sys);
                self.cached_running_status
                    .insert(app.name.clone(), is_running);
            }

            self.last_cache_update = SystemTime::now();
        }

        let panel_frame = egui::Frame {
            inner_margin: egui::Margin::symmetric(10, 10),
            ..Default::default()
        };

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut needs_save = false;
                    let mut app_to_manage = None;
                    let mut app_to_unmanage = None;

                    if self.app.apps.is_empty() {
                        ui.label("No autostart applications found.");
                    }

                    for (i, app) in self.app.apps.iter_mut().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                        }
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let icon_texture = get_or_load_icon(
                                    &mut self.texture_cache,
                                    ctx,
                                    app,
                                    self.app.os_ops.as_ref(),
                                );
                                if let Some(texture) = icon_texture {
                                    ui.add(
                                        egui::Image::new(texture).max_size(egui::vec2(20.0, 20.0)),
                                    );
                                } else {
                                    let fallback = app.name.chars().next().unwrap_or('?');
                                    ui.add_sized(
                                        [20.0, 20.0],
                                        egui::Label::new(fallback.to_string()),
                                    );
                                }
                                ui.label(egui::RichText::new(&app.name).strong());
                            });

                            let is_editing = self.editing_app_name.as_deref() == Some(&app.name);

                            if is_editing {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Command:");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.edit_buffer_command,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Working Dir:");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut self.edit_buffer_working_dir,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                    });
                                });

                                ui.horizontal(|ui| {
                                    if ui.button("Save").clicked() {
                                        app.command = self.edit_buffer_command.clone();
                                        let path_str = self.edit_buffer_working_dir.clone();
                                        app.working_dir = if path_str.is_empty() {
                                            None
                                        } else {
                                            Some(PathBuf::from(path_str))
                                        };
                                        self.editing_app_name = None;
                                        needs_save = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        self.editing_app_name = None;
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&app.command).small().monospace());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let is_running = *self
                                                .cached_running_status
                                                .get(&app.name)
                                                .unwrap_or(&false);
                                            if !is_running
                                                && ui
                                                    .button("Run")
                                                    .on_hover_text("Launch this application now.")
                                                    .clicked()
                                            {
                                                self.app.os_ops.launch_app(app);
                                            }
                                            if app.is_managed && ui.button("Edit").clicked() {
                                                self.editing_app_name = Some(app.name.clone());
                                                self.edit_buffer_command = app.command.clone();
                                                self.edit_buffer_working_dir = app
                                                    .working_dir
                                                    .as_ref()
                                                    .map(|p| p.to_string_lossy().to_string())
                                                    .unwrap_or_default();
                                            }
                                        },
                                    );
                                });

                                if let Some(wd) = &app.working_dir
                                    && !wd.as_os_str().is_empty()
                                {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Working Dir: {}",
                                            wd.display()
                                        ))
                                        .small()
                                        .monospace(),
                                    );
                                }
                            }

                            ui.separator();
                            let old_conditions = app.conditions.clone();
                            draw_condition_controls(
                                ui,
                                app,
                                &self.app.available_partitions,
                                self.app.os_ops.as_ref(),
                                self.cached_internet_ok,
                                &self.cached_disks,
                            );
                            let conditions_changed = app.conditions != old_conditions;

                            if conditions_changed {
                                let should_be_managed = app.conditions.internet
                                    || app.conditions.partition_mounted.is_some();

                                if app.is_managed {
                                    if should_be_managed {
                                        needs_save = true;
                                    } else {
                                        app_to_unmanage = Some(i);
                                    }
                                } else if should_be_managed {
                                    app_to_manage = Some(i);
                                }
                            }
                        });
                    }

                    if let Some(i) = app_to_manage
                        && self.app.os_ops.manage_app(&self.app.apps[i])
                    {
                        self.app.apps[i].is_managed = true;
                        needs_save = true;
                    }

                    if let Some(i) = app_to_unmanage
                        && self.app.os_ops.unmanage_app(&self.app.apps[i])
                    {
                        self.app.apps[i].is_managed = false;
                        needs_save = true;
                    }

                    if needs_save {
                        self.app.save_config();
                    }
                });
            });
    }
}
