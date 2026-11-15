use crate::core::preset::Preset;
use crate::core::settings::AppSettings;
use directories::ProjectDirs;
use std::path::{Path, PathBuf};

/// Errors that can occur while loading or saving settings.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Failed to read settings: {0}")]
    Read(#[from] std::io::Error),
    #[error("Failed to parse settings JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("Project directories could not be determined")]
    ProjectDirs,
}

/// Owns `AppSettings` persistence and exposes higher-level operations.
#[derive(Clone)]
pub struct SettingsService {
    settings: AppSettings,
    path: PathBuf,
}

impl SettingsService {
    /// Create a service that persists settings in the project data directory.
    pub async fn new(project_dirs: &ProjectDirs) -> Result<Self, SettingsError> {
        let data_dir = project_dirs.data_local_dir();
        tokio::fs::create_dir_all(data_dir).await?;
        let path = data_dir.join("settings.json");
        Self::load_or_default_at(path).await
    }

    /// Create a service with an explicit settings path (useful for tests).
    /// If the file does not exist, default settings are written to disk.
    pub async fn load_or_default_at(path: impl Into<PathBuf>) -> Result<Self, SettingsError> {
        let path = path.into();
        let (settings, path) = if path.exists() {
            let json = tokio::fs::read_to_string(&path).await?;
            (serde_json::from_str(&json)?, path)
        } else {
            let settings = AppSettings::default();
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let json = serde_json::to_string_pretty(&settings)?;
            tokio::fs::write(&path, json).await?;
            (settings, path)
        };
        Ok(Self { settings, path })
    }

    /// Persist the current settings to disk.
    pub async fn save(&self) -> Result<(), SettingsError> {
        let json = serde_json::to_string_pretty(&self.settings)?;
        tokio::fs::write(&self.path, json).await?;
        Ok(())
    }

    /// Access the underlying settings immutably.
    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    /// Mutable access to the underlying settings. Call `save` afterwards to
    /// persist changes.
    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    /// Path to the settings file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn game_mode(&self) -> &str {
        &self.settings.game_mode
    }

    pub async fn set_game_mode(
        &mut self,
        game_mode: impl Into<String>,
    ) -> Result<(), SettingsError> {
        self.settings.game_mode = game_mode.into();
        self.save().await
    }

    pub fn language(&self) -> &str {
        &self.settings.language
    }

    pub async fn set_language(&mut self, language: impl Into<String>) -> Result<(), SettingsError> {
        self.settings.language = language.into();
        self.save().await
    }

    pub fn is_clustered(&self) -> bool {
        self.settings.is_clustered
    }

    pub async fn set_is_clustered(&mut self, clustered: bool) -> Result<(), SettingsError> {
        self.settings.is_clustered = clustered;
        self.save().await
    }

    pub fn version_check_on_startup(&self) -> bool {
        self.settings.version_check_on_startup
    }

    pub async fn set_version_check_on_startup(
        &mut self,
        enabled: bool,
    ) -> Result<(), SettingsError> {
        self.settings.version_check_on_startup = enabled;
        self.save().await
    }

    pub fn ping_polling_enabled(&self) -> bool {
        self.settings.ping_polling_enabled
    }

    pub async fn set_ping_polling_enabled(&mut self, enabled: bool) -> Result<(), SettingsError> {
        self.settings.ping_polling_enabled = enabled;
        self.save().await
    }

    pub fn ping_polling_interval_seconds(&self) -> u64 {
        self.settings.ping_polling_interval_seconds
    }

    pub async fn set_ping_polling_interval_seconds(
        &mut self,
        seconds: u64,
    ) -> Result<(), SettingsError> {
        self.settings.ping_polling_interval_seconds = seconds.max(5);
        self.save().await
    }

    pub fn revision_for_app_id(&self, app_id: &str) -> String {
        self.settings.revision_for_app_id(app_id)
    }

    pub async fn set_revision_for_app_id(
        &mut self,
        app_id: &str,
        revision: impl Into<String>,
    ) -> Result<(), SettingsError> {
        self.settings.set_revision_for_app_id(app_id, revision);
        self.save().await
    }

    pub fn last_selected_preset_for_game_mode(&self, game_mode: &str) -> Option<&str> {
        self.settings.last_selected_preset_for_game_mode(game_mode)
    }

    pub async fn set_last_selected_preset_for_game_mode(
        &mut self,
        game_mode: &str,
        preset_name: impl Into<String>,
    ) -> Result<(), SettingsError> {
        self.settings
            .set_last_selected_preset_for_game_mode(game_mode, preset_name);
        self.save().await
    }

    pub async fn clear_last_selected_preset_for_game_mode(
        &mut self,
        game_mode: &str,
    ) -> Result<(), SettingsError> {
        self.settings
            .clear_last_selected_preset_for_game_mode(game_mode);
        self.save().await
    }

    pub fn presets_for_game_mode(&self, game_mode: &str) -> Vec<&Preset> {
        self.settings.presets_for_game_mode(game_mode)
    }

    pub fn find_preset(&self, game_mode: &str, name: &str) -> Option<&Preset> {
        self.settings.find_preset(game_mode, name)
    }

    pub async fn add_or_update_preset(&mut self, preset: Preset) -> Result<(), SettingsError> {
        self.settings.add_or_update_preset(preset);
        self.save().await
    }

    pub async fn remove_preset(
        &mut self,
        game_mode: &str,
        name: &str,
    ) -> Result<(), SettingsError> {
        self.settings.remove_preset(game_mode, name);
        self.save().await
    }

    pub async fn prune_preset_entries(
        &mut self,
        game_mode: &str,
        valid_clustered_keys: &[String],
        valid_unclustered_keys: &[String],
    ) -> Result<bool, SettingsError> {
        let changed = self.settings.prune_preset_entries(
            game_mode,
            valid_clustered_keys,
            valid_unclustered_keys,
        );
        if changed {
            self.save().await?;
        }
        Ok(changed)
    }

    pub fn has_duplicate_preset_name(&self, game_mode: &str, name: &str) -> bool {
        self.settings.has_duplicate_preset_name(game_mode, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_service() -> (SettingsService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::load_or_default_at(dir.path().join("settings.json"))
            .await
            .unwrap();
        (service, dir)
    }

    #[tokio::test]
    async fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        {
            let mut service = SettingsService::load_or_default_at(&path).await.unwrap();
            service.set_game_mode("Deadlock").await.unwrap();
            service.set_language("Russian | ru-ru").await.unwrap();
            service.set_is_clustered(true).await.unwrap();
            service
                .set_revision_for_app_id("1422450", "rev-42")
                .await
                .unwrap();
            service
                .set_last_selected_preset_for_game_mode("Deadlock", "My Preset")
                .await
                .unwrap();
        }

        let loaded = SettingsService::load_or_default_at(&path).await.unwrap();
        assert_eq!(loaded.game_mode(), "Deadlock");
        assert_eq!(loaded.language(), "Russian | ru-ru");
        assert!(loaded.is_clustered());
        assert_eq!(loaded.revision_for_app_id("1422450"), "rev-42");
        assert_eq!(
            loaded.last_selected_preset_for_game_mode("Deadlock"),
            Some("My Preset")
        );
    }

    #[tokio::test]
    async fn missing_file_uses_defaults() {
        let (service, _dir) = temp_service().await;
        assert_eq!(service.game_mode(), "Counter Strike 2");
        assert!(service.path().exists());
    }

    #[tokio::test]
    async fn add_preset_and_persist() {
        let (mut service, _dir) = temp_service().await;

        let mut preset = Preset::new("EU", "Counter Strike 2");
        preset.blocked_server_keys = vec!["hkg".to_string()];
        service.add_or_update_preset(preset).await.unwrap();

        let loaded = SettingsService::load_or_default_at(service.path())
            .await
            .unwrap();
        let stored = loaded.find_preset("Counter Strike 2", "EU").unwrap();
        assert_eq!(stored.blocked_server_keys, vec!["hkg"]);
    }

    #[tokio::test]
    async fn prune_presets_persists_changes() {
        let (mut service, _dir) = temp_service().await;

        let mut preset = Preset::new("Pruned", "Counter Strike 2");
        preset.is_clustered = true;
        preset.blocked_server_keys = vec!["keep".to_string(), "remove".to_string()];
        service.add_or_update_preset(preset).await.unwrap();

        let changed = service
            .prune_preset_entries("Counter Strike 2", &["keep".to_string()], &[])
            .await
            .unwrap();
        assert!(changed);

        let loaded = SettingsService::load_or_default_at(service.path())
            .await
            .unwrap();
        let stored = loaded.find_preset("Counter Strike 2", "Pruned").unwrap();
        assert_eq!(stored.blocked_server_keys, vec!["keep"]);
    }
}
