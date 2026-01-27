use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct ZellijTabSync {
    /// Previously active tab name (to avoid redundant notifications)
    prev_active_tab: Option<String>,
}

register_plugin!(ZellijTabSync);

impl ZellijPlugin for ZellijTabSync {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[EventType::TabUpdate]);
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        if let Event::TabUpdate(tabs) = event {
            if let Some(active_tab) = tabs.iter().find(|t| t.active) {
                let tab_name = active_tab.name.clone();

                // Only notify if the active tab has changed
                if self.prev_active_tab.as_ref() != Some(&tab_name) {
                    self.prev_active_tab = Some(tab_name.clone());
                    run_command(
                        &["workspace-manager", "notify", "tab-focus", &tab_name],
                        BTreeMap::new(),
                    );
                }
            }
        }
        false
    }
}
