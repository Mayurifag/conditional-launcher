use crate::app::{perform_launch_checks, ConditionalLauncherApp};
use crate::os::get_os_operations;
use std::time::Duration;

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
}
