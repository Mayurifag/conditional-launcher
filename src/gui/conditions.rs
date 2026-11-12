use crate::config::AppConfig;
use crate::os::{OsOperations, PartitionInfo};
use eframe::egui;
use sysinfo::Disks;

pub(super) fn draw(
    ui: &mut egui::Ui,
    app: &mut AppConfig,
    partitions: &[PartitionInfo],
    os_ops: &dyn OsOperations,
    has_internet: bool,
    disks: &Disks,
) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.conditions.internet, "Internet")
            .on_hover_text(
                "If checked, this app will only launch if there is an active internet connection.",
            );
        if app.conditions.internet {
            ui.label(if has_internet { "✅" } else { "❌" })
                .on_hover_text("Current internet connection status.");
        }
        ui.separator();
        ui.label("Partition:").on_hover_text(
            "If a partition is selected, this app will only launch if that partition is mounted.",
        );
        egui::ComboBox::from_id_salt(&app.name)
            .selected_text(
                app.conditions
                    .partition_mounted
                    .as_deref()
                    .unwrap_or("None"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.conditions.partition_mounted, None, "None");
                for partition in partitions {
                    let details = if partition.fs_type.is_empty() {
                        partition.size.clone()
                    } else {
                        format!("{}, {}", partition.fs_type, partition.size)
                    };
                    ui.selectable_value(
                        &mut app.conditions.partition_mounted,
                        Some(partition.mount_point.clone()),
                        format!("{} ({details})", partition.mount_point),
                    );
                }
            });
        if app.conditions.partition_mounted.is_some() {
            let status = app.conditions.check(os_ops, has_internet, disks);
            ui.label(if status.partition_ok { "✅" } else { "❌" })
                .on_hover_text("Current status of the selected partition.");
        }
    });
}
