use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::core::app_paths::{self, AppPaths};
use crate::core::model_catalog::ModelCatalog;
use crate::core::models::{AppSettings, EnvironmentSnapshot, HardwareSnapshot, TaskSnapshot};
use crate::core::settings::SettingsStore;

pub struct StateView {
    settings: SettingsStore,
    catalog: Arc<ModelCatalog>,
    hardware: Mutex<HardwareSnapshot>,
    environment: Mutex<EnvironmentSnapshot>,
    task: Mutex<TaskSnapshot>,
}

impl StateView {
    pub fn new(settings: SettingsStore, catalog: Arc<ModelCatalog>) -> Self {
        Self {
            settings,
            catalog,
            hardware: Mutex::new(HardwareSnapshot::default()),
            environment: Mutex::new(EnvironmentSnapshot::with_directories(
                AppPaths::runtime_directory().to_string_lossy().into_owned(),
                AppPaths::model_cache_directory()
                    .to_string_lossy()
                    .into_owned(),
            )),
            task: Mutex::new(TaskSnapshot::idle()),
        }
    }

    pub fn settings_store(&self) -> &SettingsStore {
        &self.settings
    }

    pub fn catalog(&self) -> Arc<ModelCatalog> {
        Arc::clone(&self.catalog)
    }

    pub fn set_hardware(&self, value: HardwareSnapshot) {
        if let Ok(mut guard) = self.hardware.lock() {
            *guard = value;
        }
    }

    pub fn hardware(&self) -> HardwareSnapshot {
        self.hardware
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set_environment(&self, value: EnvironmentSnapshot) {
        if let Ok(mut guard) = self.environment.lock() {
            *guard = value;
        }
    }

    pub fn set_task(&self, value: TaskSnapshot) {
        if let Ok(mut guard) = self.task.lock() {
            *guard = value;
        }
    }

    pub fn task(&self) -> TaskSnapshot {
        self.task
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| TaskSnapshot::idle())
    }

    pub fn build(&self) -> serde_json::Value {
        let settings: AppSettings = self.settings.current();
        let output_directory = settings
            .output_directory
            .clone()
            .unwrap_or_else(|| app_paths::user_profile().to_string_lossy().into_owned());
        let services = settings
            .services
            .iter()
            .map(|service| {
                json!({
                    "id": service.id,
                    "name": service.name,
                    "address": service.address,
                    "type": service.kind,
                    "userId": service.user_id,
                    "hasCredential": service.has_credential(),
                })
            })
            .collect::<Vec<_>>();
        let issue = self.settings.last_load_issue();
        let exists = self.settings.exists();
        json!({
            "settings": {
                "language": settings.language,
                "theme": settings.theme,
                "device": settings.device,
                "outputDirectory": output_directory,
            },
            "hasSavedSettings": exists && issue.is_none(),
            "settingsStatus": issue.clone().unwrap_or_else(|| match exists {
                true => "loaded".to_string(),
                false => "not-created".to_string(),
            }),
            "services": services,
            "hardware": self.hardware(),
            "environment": self
                .environment
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default(),
            "models": self.catalog.snapshots(),
            "task": self.task(),
            "dataDirectory": AppPaths::data_directory().to_string_lossy().into_owned(),
            "settingsPath": self.settings.path_string(),
            "logPath": AppPaths::log_path().to_string_lossy().into_owned(),
            "catalogIssue": self.catalog.last_load_issue(),
            "lastError": issue,
            "version": env!("CARGO_PKG_VERSION"),
            "runtimeVersion": env!("APP_RUST_VERSION"),
            "installPath": install_path(),
        })
    }
}

fn install_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|value| value.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}
