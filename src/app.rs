use crate::config::{AppConfig, Config};
use crate::os::{OsOperations, PartitionInfo, get_os_operations};
use eframe::egui;
use freedesktop_icons as icons;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use sysinfo::{Disks, ProcessRefreshKind, RefreshKind, System};

pub struct ConditionStatus {
    pub internet_ok: bool,
    pub partition_ok: bool,
}

fn check_app_conditions(
    os_ops: &dyn OsOperations,
    app: &AppConfig,
    has_internet: bool,
    disks: &Disks,
) -> ConditionStatus {
    let partition_ok = app
        .conditions
        .partition_mounted
        .as_ref()
        .map_or(true, |p| os_ops.is_partition_mounted(p, disks));
    let internet_ok = !app.conditions.internet || has_internet;
    ConditionStatus {
        internet_ok,
        partition_ok,
    }
}

pub fn perform_launch_checks(os_ops: &dyn OsOperations, managed_apps: &mut [AppConfig]) {
    let has_internet = os_ops.check_internet_connection();
    let mut disks = Disks::new();
    disks.refresh(true);

    for app in managed_apps.iter_mut() {
        if app.launched {
            continue;
        }

        let status = check_app_conditions(os_ops, app, has_internet, &disks);

        if status.internet_ok && status.partition_ok {
            os_ops.launch_app(app);
            app.launched = true;
        }
    }
}

pub fn run_hidden_process() {
    let os_ops = get_os_operations();
    let mut managed_apps = ConditionalLauncherApp::load_config();
    let total_apps_to_launch = managed_apps.len();

    if total_apps_to_launch == 0 {
        return;
    }

    let mut launched_app_names: Vec<String> = Vec::new();

    loop {
        perform_launch_checks(os_ops.as_ref(), &mut managed_apps);

        for app in managed_apps.iter().filter(|a| a.launched) {
            if !launched_app_names.contains(&app.name) {
                launched_app_names.push(app.name.clone());
            }
        }

        if launched_app_names.len() >= total_apps_to_launch {
            break;
        }

        std::thread::sleep(Duration::from_secs(5));
    }

    os_ops.send_exit_notification(&launched_app_names);
}
pub struct ConditionalLauncherApp {
    apps: Vec<AppConfig>,
    os_ops: Box<dyn OsOperations>,
    available_partitions: Vec<PartitionInfo>,
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

pub fn load_all_apps(os_ops: &dyn OsOperations) -> Vec<AppConfig> {
    let mut managed_apps = ConditionalLauncherApp::load_config();
    for app in &mut managed_apps {
        app.is_managed = true;
    }

    let unmanaged_apps = os_ops.get_autostart_apps();

    let mut apps = managed_apps;
    for unmanaged_app in unmanaged_apps {
        if !apps.iter().any(|a| a.name == unmanaged_app.name) {
            apps.push(unmanaged_app);
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn load_icon<'a>(
    texture_cache: &'a mut HashMap<String, egui::TextureHandle>,
    ctx: &egui::Context,
    icon_name: &str,
) -> Option<&'a egui::TextureHandle> {
    if texture_cache.contains_key(icon_name) {
        return texture_cache.get(icon_name);
    }

    if let Some(path) = icons::lookup(icon_name).with_size(32).find() {
        if let Ok(image_data) = fs::read(&path) {
            let color_image = if path.extension().map_or(false, |e| e == "svg") {
                let rtree = usvg::Tree::from_data(&image_data, &usvg::Options::default()).ok()?;
                let svg_size = rtree.size();
                let (width, height) = (32, 32);
                let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
                let sx = width as f32 / svg_size.width();
                let sy = height as f32 / svg_size.height();
                let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
                resvg::render(&rtree, transform, &mut pixmap.as_mut());
                Some(egui::ColorImage::from_rgba_unmultiplied(
                    [pixmap.width() as usize, pixmap.height() as usize],
                    pixmap.data(),
                ))
            } else {
                image::load_from_memory(&image_data).ok().map(|image| {
                    let image_rgba = image.to_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    egui::ColorImage::from_rgba_unmultiplied(size, &image_rgba.into_raw())
                })
            };

            if let Some(color_image) = color_image {
                let texture =
                    ctx.load_texture(icon_name.to_string(), color_image, Default::default());
                texture_cache.insert(icon_name.to_string(), texture);
                return texture_cache.get(icon_name);
            }
        }
    }
    None
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
                        format!("{} {}", display_label, details),
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

impl ConditionalLauncherApp {
    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap()
            .join("conditional-launcher/managed_apps.toml")
    }

    pub fn load_config() -> Vec<AppConfig> {
        fs::read_to_string(Self::config_path())
            .ok()
            .and_then(|toml_str| toml::from_str::<Config>(&toml_str).ok())
            .map(|config| config.apps)
            .unwrap_or_default()
    }

    fn save_config(&mut self) {
        let managed_apps: Vec<_> = self.apps.iter().filter(|a| a.is_managed).cloned().collect();

        let config = Config { apps: managed_apps };
        if let Some(parent) = Self::config_path().parent() {
            fs::create_dir_all(parent).ok();
        }
        let toml = toml::to_string_pretty(&config).unwrap();
        fs::write(Self::config_path(), toml).ok();

        let managed_app_count = config.apps.len();
        if managed_app_count == 0 {
            self.os_ops.remove_self_from_autostart();
        } else {
            self.os_ops.add_self_to_autostart(managed_app_count);
        }
    }

    pub fn new(_cc: &eframe::CreationContext<'_>, apps: Vec<AppConfig>) -> Self {
        let os_ops = crate::os::get_os_operations();
        let available_partitions = os_ops.get_partitions();

        Self {
            apps,
            os_ops,
            available_partitions,
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
        let fresh_unmanaged = self.os_ops.get_autostart_apps();

        self.apps
            .retain(|app| app.is_managed || fresh_unmanaged.iter().any(|u| u.name == app.name));

        for app in fresh_unmanaged {
            if !self.apps.iter().any(|a| a.name == app.name) {
                self.apps.push(app);
            }
        }

        self.apps
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }
}

impl eframe::App for ConditionalLauncherApp {
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
            self.cached_internet_ok = self.os_ops.check_internet_connection();
            self.cached_disks.refresh(true);

            self.sys.refresh_specifics(
                RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
            );

            let autostart_path = dirs::config_dir().unwrap().join("autostart");
            if let Ok(metadata) = fs::metadata(&autostart_path) {
                if let Ok(mod_time) = metadata.modified() {
                    if self.last_autostart_check.map_or(true, |t| t != mod_time) {
                        self.refresh_autostart_list();
                        self.last_autostart_check = Some(mod_time);
                    }
                }
            }

            self.cached_running_status.clear();
            for app in self.apps.iter() {
                let is_running = self.os_ops.is_app_running(app, &self.sys);
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

                    if self.apps.is_empty() {
                        ui.label("No autostart applications found.");
                    }

                    for (i, app) in self.apps.iter_mut().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                        }
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut icon_shown = false;
                                if let Some(icon_name) = &app.icon {
                                    if let Some(texture) =
                                        load_icon(&mut self.texture_cache, ctx, icon_name)
                                    {
                                        ui.add(
                                            egui::Image::new(texture)
                                                .max_size(egui::vec2(20.0, 20.0)),
                                        );
                                        icon_shown = true;
                                    }
                                }
                                if !icon_shown {
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
                                            if !is_running {
                                                if ui
                                                    .button("Run")
                                                    .on_hover_text("Launch this application now.")
                                                    .clicked()
                                                {
                                                    self.os_ops.launch_app(app);
                                                }
                                            }
                                            if app.is_managed {
                                                if ui.button("Edit").clicked() {
                                                    self.editing_app_name = Some(app.name.clone());
                                                    self.edit_buffer_command = app.command.clone();
                                                    self.edit_buffer_working_dir = app
                                                        .working_dir
                                                        .as_ref()
                                                        .map(|p| p.to_string_lossy().to_string())
                                                        .unwrap_or_default();
                                                }
                                            }
                                        },
                                    );
                                });

                                if let Some(wd) = &app.working_dir {
                                    if !wd.as_os_str().is_empty() {
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
                            }

                            ui.separator();
                            let old_conditions = app.conditions.clone();
                            draw_condition_controls(
                                ui,
                                app,
                                &self.available_partitions,
                                self.os_ops.as_ref(),
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

                    if let Some(i) = app_to_manage {
                        if self.os_ops.manage_app(&self.apps[i]) {
                            self.apps[i].is_managed = true;
                            needs_save = true;
                        }
                    }

                    if let Some(i) = app_to_unmanage {
                        if self.os_ops.unmanage_app(&self.apps[i]) {
                            self.apps[i].is_managed = false;
                            needs_save = true;
                        }
                    }

                    if needs_save {
                        self.save_config();
                    }
                });
            });
    }
}
