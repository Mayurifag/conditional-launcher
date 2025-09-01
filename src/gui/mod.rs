slint::include_modules!();

use crate::app::ConditionalLauncherApp;
use crate::config::AppConfig as RustAppConfig;
use crate::os::PartitionInfo as OsPartitionInfo;
use freedesktop_icons::lookup;
use image::ImageReader;
use slint::{Image, Model, SharedPixelBuffer, VecModel};
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;

fn load_icon(icon_name: &Option<String>) -> Image {
    // Create a 1x1 transparent fallback icon
    let fallback_data = vec![0u8; 4]; // RGBA with all zeros (transparent)
    let fallback_icon =
        Image::from_rgba8(SharedPixelBuffer::clone_from_slice(&fallback_data, 1, 1));

    let icon_name = match icon_name {
        Some(name) => name,
        None => return fallback_icon,
    };

    let icon_path = if icon_name.starts_with('/') {
        Some(icon_name.into())
    } else {
        lookup(icon_name)
            .with_size(48)
            .with_theme("breeze-dark")
            .find()
    };

    if let Some(path) = icon_path {
        if let Ok(img_data) = std::fs::read(&path) {
            if path.extension().map_or(false, |e| e == "svg") {
                // Handle SVG files
                if let Ok(tree) = usvg::Tree::from_data(&img_data, &usvg::Options::default()) {
                    let size = tree.size();
                    let target_size = 48.0;
                    let scale = target_size / size.width().max(size.height());
                    let width = (size.width() * scale) as u32;
                    let height = (size.height() * scale) as u32;

                    if let Some(mut pixmap) = tiny_skia::Pixmap::new(width, height) {
                        let transform = tiny_skia::Transform::from_scale(scale, scale);
                        resvg::render(&tree, transform, &mut pixmap.as_mut());
                        let buffer = SharedPixelBuffer::clone_from_slice(
                            pixmap.data(),
                            pixmap.width(),
                            pixmap.height(),
                        );
                        return Image::from_rgba8(buffer);
                    }
                }
            } else {
                // Handle raster images
                if let Ok(reader) = ImageReader::new(Cursor::new(&img_data)).with_guessed_format() {
                    if let Ok(image) = reader.decode() {
                        let rgba_image = image.to_rgba8();
                        let buffer = SharedPixelBuffer::clone_from_slice(
                            rgba_image.as_raw(),
                            rgba_image.width(),
                            rgba_image.height(),
                        );
                        return Image::from_rgba8(buffer);
                    }
                }
            }
        }
    }
    fallback_icon
}

fn to_slint_app(app: &RustAppConfig) -> AppConfig {
    AppConfig {
        name: app.name.clone().into(),
        is_managed: app.is_managed,
        conditions: Conditions {
            internet: app.conditions.internet,
            partition_mounted: app
                .conditions
                .partition_mounted
                .clone()
                .unwrap_or_default()
                .into(),
        },
        icon: load_icon(&app.icon),
    }
}

fn to_slint_partition(p: &OsPartitionInfo) -> PartitionInfo {
    PartitionInfo {
        mount_point: p.mount_point.clone().into(),
        fs_type: p.fs_type.clone().into(),
        size: p.size.clone().into(),
    }
}

pub fn run(app: &mut ConditionalLauncherApp) -> Result<(), slint::PlatformError> {
    let main_window = MainWindow::new()?;

    let slint_partitions: Vec<PartitionInfo> = app
        .available_partitions
        .iter()
        .map(to_slint_partition)
        .collect();
    main_window.set_available_partitions(Rc::new(VecModel::from(slint_partitions)).into());

    // Create partition options for ComboBox
    let mut partition_options = vec!["None".into()];
    for partition in &app.available_partitions {
        partition_options.push(partition.mount_point.clone().into());
    }
    main_window.set_partition_options(Rc::new(VecModel::from(partition_options)).into());

    let slint_apps: Vec<AppConfig> = app.apps.iter().map(to_slint_app).collect();
    let apps_model = Rc::new(VecModel::from(slint_apps));
    main_window.set_apps(apps_model.into());

    main_window.on_save_config(|| {});

    let main_window_weak = main_window.as_weak();
    main_window.on_close_window(move || {
        if let Some(window) = main_window_weak.upgrade() {
            window.window().hide().unwrap();
        }
    });

    main_window.run()?;

    let final_slint_apps = main_window.get_apps();

    let mut original_apps_map: HashMap<String, RustAppConfig> =
        app.apps.drain(..).map(|a| (a.name.clone(), a)).collect();

    for slint_app in final_slint_apps.iter() {
        if let Some(original_app) = original_apps_map.get_mut(slint_app.name.as_str()) {
            let was_managed = original_app.is_managed;
            original_app.is_managed = slint_app.is_managed;
            original_app.conditions.internet = slint_app.conditions.internet;
            original_app.conditions.partition_mounted =
                if slint_app.conditions.partition_mounted.is_empty() {
                    None
                } else {
                    Some(slint_app.conditions.partition_mounted.to_string())
                };

            if was_managed != original_app.is_managed {
                if original_app.is_managed {
                    app.os_ops.manage_app(original_app);
                } else {
                    app.os_ops.unmanage_app(original_app);
                }
            }
        }
    }

    app.apps = original_apps_map.into_values().collect();
    app.apps
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    app.save_config();

    Ok(())
}
