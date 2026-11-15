use crate::core::preset::Preset;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_WARNING: &str = "Do not modify settings here! only do it from the app!";
const DEFAULT_GAME_MODE: &str = "Counter Strike 2";
const DEFAULT_LANGUAGE: &str = "English | en-us";

/// Persistent application settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub warning: String,
    #[serde(rename = "gameMode")]
    pub game_mode: String,
    pub language: String,
    #[serde(rename = "groupByRegion", default)]
    pub group_by_region: bool,
    #[serde(rename = "versionCheckOnStartup")]
    pub version_check_on_startup: bool,
    #[serde(rename = "pingPollingEnabled")]
    pub ping_polling_enabled: bool,
    #[serde(rename = "pingPollingIntervalSeconds")]
    pub ping_polling_interval_seconds: u64,
    #[serde(rename = "serverRevisions")]
    pub server_revisions: HashMap<String, String>,
    #[serde(rename = "lastSelectedPresetNames")]
    pub last_selected_preset_names: HashMap<String, String>,
    pub presets: Vec<Preset>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            warning: DEFAULT_WARNING.to_string(),
            game_mode: DEFAULT_GAME_MODE.to_string(),
            language: DEFAULT_LANGUAGE.to_string(),
            group_by_region: true,
            version_check_on_startup: true,
            ping_polling_enabled: true,
            ping_polling_interval_seconds: 15,
            server_revisions: HashMap::new(),
            last_selected_preset_names: HashMap::new(),
            presets: Vec::new(),
        }
    }
}

impl AppSettings {
    /// Get the stored revision for an app id, or "-1" if none exists.
    pub fn revision_for_app_id(&self, app_id: &str) -> String {
        self.server_revisions
            .get(&app_id.to_lowercase())
            .cloned()
            .unwrap_or_else(|| "-1".to_string())
    }

    /// Set the revision for an app id.
    pub fn set_revision_for_app_id(&mut self, app_id: &str, revision: impl Into<String>) {
        self.server_revisions
            .insert(app_id.to_lowercase(), revision.into());
    }

    /// Get the last selected preset name for a game mode.
    pub fn last_selected_preset_for_game_mode(&self, game_mode: &str) -> Option<&str> {
        self.last_selected_preset_names
            .get(&game_mode.to_lowercase())
            .map(|s| s.as_str())
    }

    /// Set the last selected preset name for a game mode.
    pub fn set_last_selected_preset_for_game_mode(
        &mut self,
        game_mode: &str,
        preset_name: impl Into<String>,
    ) {
        self.last_selected_preset_names
            .insert(game_mode.to_lowercase(), preset_name.into());
    }

    /// Clear the last selected preset name for a game mode.
    pub fn clear_last_selected_preset_for_game_mode(&mut self, game_mode: &str) {
        self.last_selected_preset_names
            .remove(&game_mode.to_lowercase());
    }

    /// Find a preset by game mode and name (case-insensitive).
    pub fn find_preset(&self, game_mode: &str, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|preset| preset.matches(game_mode, name))
    }

    /// Find a mutable preset by game mode and name (case-insensitive).
    pub fn find_preset_mut(&mut self, game_mode: &str, name: &str) -> Option<&mut Preset> {
        self.presets
            .iter_mut()
            .find(|preset| preset.matches(game_mode, name))
    }

    /// Get all presets for a game mode (case-insensitive).
    pub fn presets_for_game_mode(&self, game_mode: &str) -> Vec<&Preset> {
        self.presets
            .iter()
            .filter(|preset| preset.game_mode.eq_ignore_ascii_case(game_mode))
            .collect()
    }

    /// Add a new preset or update an existing one.
    pub fn add_or_update_preset(&mut self, mut preset: Preset) {
        preset.normalize_blocked_keys();

        if let Some(existing) = self.find_preset_mut(&preset.game_mode, &preset.name) {
            existing.search_text = preset.search_text;
            existing.blocked_server_keys = preset.blocked_server_keys;
        } else {
            self.presets.push(preset);
        }
    }

    /// Remove a preset by game mode and name.
    pub fn remove_preset(&mut self, game_mode: &str, name: &str) {
        self.presets
            .retain(|preset| !preset.matches(game_mode, name));
    }

    /// Remove blocked server keys from presets when Valve removes servers.
    /// Returns `true` if any preset was modified.
    pub fn prune_preset_entries(
        &mut self,
        game_mode: &str,
        valid_keys: &[String],
    ) -> bool {
        let valid: std::collections::HashSet<String> =
            valid_keys.iter().map(|k| k.to_lowercase()).collect();

        let mut changed = false;

        for preset in self
            .presets
            .iter_mut()
            .filter(|p| p.game_mode.eq_ignore_ascii_case(game_mode))
        {
            let original_len = preset.blocked_server_keys.len();
            preset.blocked_server_keys = preset
                .blocked_server_keys
                .iter()
                .filter(|key| valid.contains(&key.to_lowercase()))
                .cloned()
                .collect();
            preset.normalize_blocked_keys();

            if preset.blocked_server_keys.len() != original_len {
                changed = true;
            }
        }

        changed
    }

    /// Check whether more than one preset shares the same game mode and name.
    pub fn has_duplicate_preset_name(&self, game_mode: &str, name: &str) -> bool {
        self.presets
            .iter()
            .filter(|preset| preset.matches(game_mode, name))
            .count()
            > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_values_match_csharp_defaults() {
        let settings = AppSettings::default();
        assert_eq!(settings.warning, DEFAULT_WARNING);
        assert_eq!(settings.game_mode, DEFAULT_GAME_MODE);
        assert_eq!(settings.language, DEFAULT_LANGUAGE);
        assert!(settings.version_check_on_startup);
    }

    #[test]
    fn revision_round_trips_through_app_id() {
        let mut settings = AppSettings::default();
        assert_eq!(settings.revision_for_app_id("730"), "-1");
        settings.set_revision_for_app_id("730", "abc123");
        assert_eq!(settings.revision_for_app_id("730"), "abc123");
        // Case-insensitive key lookup.
        assert_eq!(settings.revision_for_app_id("730"), "abc123");
    }

    #[test]
    fn last_selected_preset_round_trips() {
        let mut settings = AppSettings::default();
        assert!(settings
            .last_selected_preset_for_game_mode("Counter Strike 2")
            .is_none());
        settings.set_last_selected_preset_for_game_mode("Counter Strike 2", "EU Only");
        assert_eq!(
            settings.last_selected_preset_for_game_mode("counter strike 2"),
            Some("EU Only")
        );
        settings.clear_last_selected_preset_for_game_mode("Counter Strike 2");
        assert!(settings
            .last_selected_preset_for_game_mode("Counter Strike 2")
            .is_none());
    }

    #[test]
    fn add_or_update_preset_creates_and_updates() {
        let mut settings = AppSettings::default();

        let mut preset = Preset::new("EU Only", "Counter Strike 2");
        preset.blocked_server_keys = vec!["hkg".to_string(), "sto".to_string()];
        settings.add_or_update_preset(preset.clone());

        assert_eq!(settings.presets.len(), 1);
        let stored = settings.find_preset("Counter Strike 2", "EU Only").unwrap();
        assert_eq!(stored.blocked_server_keys, vec!["hkg", "sto"]);

        preset.blocked_server_keys = vec!["hkg".to_string()];
        settings.add_or_update_preset(preset);

        assert_eq!(settings.presets.len(), 1);
        let stored = settings.find_preset("Counter Strike 2", "EU Only").unwrap();
        assert_eq!(stored.blocked_server_keys, vec!["hkg"]);
    }

    #[test]
    fn remove_preset_deletes_only_matching_preset() {
        let mut settings = AppSettings::default();
        settings.add_or_update_preset(Preset::new("A", "Counter Strike 2"));
        settings.add_or_update_preset(Preset::new("B", "Counter Strike 2"));
        settings.add_or_update_preset(Preset::new("A", "Deadlock"));

        settings.remove_preset("Counter Strike 2", "A");

        assert_eq!(settings.presets.len(), 2);
        assert!(settings.find_preset("Counter Strike 2", "A").is_none());
        assert!(settings.find_preset("Counter Strike 2", "B").is_some());
        assert!(settings.find_preset("Deadlock", "A").is_some());
    }

    #[test]
    fn prune_preset_entries_removes_stale_keys() {
        let mut settings = AppSettings::default();

        let mut preset = Preset::new("EU Only", "Counter Strike 2");
        preset.blocked_server_keys = vec!["hkg".to_string(), "old".to_string()];
        settings.add_or_update_preset(preset);

        let changed = settings.prune_preset_entries(
            "Counter Strike 2",
            &["hkg".to_string()],
        );

        assert!(changed);

        let preset = settings
            .find_preset("Counter Strike 2", "EU Only")
            .unwrap();
        assert_eq!(preset.blocked_server_keys, vec!["hkg"]);
    }

    #[test]
    fn has_duplicate_preset_name_detects_duplicates() {
        let mut settings = AppSettings::default();
        // Insert directly because add_or_update_preset merges matching names.
        settings.presets.push(Preset::new("A", "Counter Strike 2"));
        settings.presets.push(Preset::new("a", "Counter Strike 2"));
        assert!(settings.has_duplicate_preset_name("Counter Strike 2", "A"));
        assert!(!settings.has_duplicate_preset_name("Counter Strike 2", "B"));
    }
}
