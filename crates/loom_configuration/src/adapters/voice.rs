use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

use super::ConfigAdapter;
use crate::{
    ManagedAppId, ManagedConfigError, UiField, UiFieldKind, UiFieldOption, UiSection,
    ValidationError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    Dictate,
    Polish,
    Translate,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    Toggle,
    PushToTalk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Mock,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    ClipboardPaste,
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardBackendMode {
    Fallback,
    NativeWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackendMode {
    Silent,
    NativeWindows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub trigger: TriggerConfig,
    pub audio: AudioConfig,
    pub provider: ProviderConfig,
    pub output: OutputConfig,
    pub logging: LoggingConfig,
    pub voice_mode: VoiceMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub mode: TriggerMode,
    pub toggle_shortcut: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    pub backend: AudioBackendMode,
    pub max_recording_seconds: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub temp_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub mock_transcript: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub restore_clipboard: bool,
    pub clipboard_backend: ClipboardBackendMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub dir: PathBuf,
}

pub struct HookVoiceAdapter;
pub struct TalkVoiceAdapter;

impl ConfigAdapter for HookVoiceAdapter {
    fn app(&self) -> ManagedAppId {
        ManagedAppId::Hook
    }

    fn display_name(&self) -> &'static str {
        "Hook"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> Value {
        voice_default_value("arthook")
    }

    fn normalize_and_validate(&self, value: Value) -> Result<Value, ManagedConfigError> {
        normalize_voice(value)
    }

    fn ui_sections(&self, value: &Value) -> Vec<UiSection> {
        voice_ui_sections(value)
    }
}

impl ConfigAdapter for TalkVoiceAdapter {
    fn app(&self) -> ManagedAppId {
        ManagedAppId::Talk
    }

    fn display_name(&self) -> &'static str {
        "Talk"
    }

    fn schema_version(&self) -> u32 {
        1
    }

    fn default_config(&self) -> Value {
        voice_default_value("talk")
    }

    fn normalize_and_validate(&self, value: Value) -> Result<Value, ManagedConfigError> {
        normalize_voice(value)
    }

    fn ui_sections(&self, value: &Value) -> Vec<UiSection> {
        voice_ui_sections(value)
    }
}

fn voice_default_value(app: &str) -> Value {
    let root = PathBuf::from(".runtime")
        .join("neuro")
        .join(app)
        .join("voice");
    serde_json::to_value(VoiceConfig {
        trigger: TriggerConfig {
            mode: TriggerMode::Toggle,
            toggle_shortcut: "Ctrl+Alt+Space".to_string(),
        },
        audio: AudioConfig {
            backend: AudioBackendMode::Silent,
            max_recording_seconds: 60,
            sample_rate_hz: 16000,
            channels: 1,
            temp_dir: root.join("audio"),
        },
        provider: ProviderConfig {
            kind: ProviderKind::Mock,
            mock_transcript: Some(format!("hello from {app} voice")),
            endpoint: None,
        },
        output: OutputConfig {
            mode: OutputMode::DryRun,
            restore_clipboard: true,
            clipboard_backend: ClipboardBackendMode::Fallback,
        },
        logging: LoggingConfig {
            dir: root.join("logs"),
        },
        voice_mode: VoiceMode::Dictate,
    })
    .expect("serialize voice default config")
}

fn normalize_voice(value: Value) -> Result<Value, ManagedConfigError> {
    let config: VoiceConfig = serde_json::from_value(value).map_err(|error| {
        ManagedConfigError::invalid(vec![ValidationError::new("$", error.to_string())])
    })?;
    validate_voice(&config)?;
    serde_json::to_value(config).map_err(|error| {
        ManagedConfigError::invalid(vec![ValidationError::new("$", error.to_string())])
    })
}

fn validate_voice(config: &VoiceConfig) -> Result<(), ManagedConfigError> {
    let mut errors = Vec::new();
    if config.trigger.toggle_shortcut.trim().is_empty() {
        errors.push(ValidationError::new(
            "trigger.toggle_shortcut",
            "must not be empty",
        ));
    }
    if config.audio.max_recording_seconds == 0 {
        errors.push(ValidationError::new(
            "audio.max_recording_seconds",
            "must be greater than 0",
        ));
    }
    if config.audio.sample_rate_hz == 0 {
        errors.push(ValidationError::new(
            "audio.sample_rate_hz",
            "must be greater than 0",
        ));
    }
    if config.audio.channels == 0 {
        errors.push(ValidationError::new(
            "audio.channels",
            "must be greater than 0",
        ));
    }
    if config.audio.temp_dir.as_os_str().is_empty() {
        errors.push(ValidationError::new("audio.temp_dir", "must not be empty"));
    }
    if config.logging.dir.as_os_str().is_empty() {
        errors.push(ValidationError::new("logging.dir", "must not be empty"));
    }
    if config.provider.kind == ProviderKind::Mock
        && config
            .provider
            .mock_transcript
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        errors.push(ValidationError::new(
            "provider.mock_transcript",
            "must be set for mock provider",
        ));
    }
    if config.provider.kind == ProviderKind::Http
        && config
            .provider
            .endpoint
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        errors.push(ValidationError::new(
            "provider.endpoint",
            "must be set for http provider",
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ManagedConfigError::invalid(errors))
    }
}

fn voice_ui_sections(value: &Value) -> Vec<UiSection> {
    vec![
        UiSection {
            title: "Trigger".to_string(),
            fields: vec![
                select_field(
                    value,
                    "trigger.mode",
                    "Trigger mode",
                    &["toggle", "push_to_talk"],
                ),
                text_field(value, "trigger.toggle_shortcut", "Toggle shortcut"),
            ],
        },
        UiSection {
            title: "Provider".to_string(),
            fields: vec![
                select_field(value, "provider.kind", "Provider", &["mock", "http"]),
                text_field(value, "provider.mock_transcript", "Mock transcript"),
                text_field(value, "provider.endpoint", "HTTP endpoint"),
            ],
        },
        UiSection {
            title: "Voice mode".to_string(),
            fields: vec![select_field(
                value,
                "voice_mode",
                "Default voice mode",
                &["dictate", "polish", "translate", "command"],
            )],
        },
    ]
}

fn text_field(value: &Value, path: &str, label: &str) -> UiField {
    UiField {
        path: path.to_string(),
        label: label.to_string(),
        kind: UiFieldKind::Text,
        options: Vec::new(),
        value: value_at_path(value, path),
    }
}

fn select_field(value: &Value, path: &str, label: &str, options: &[&str]) -> UiField {
    UiField {
        path: path.to_string(),
        label: label.to_string(),
        kind: UiFieldKind::Select,
        options: options
            .iter()
            .map(|option| UiFieldOption {
                value: (*option).to_string(),
                label: (*option).to_string(),
            })
            .collect(),
        value: value_at_path(value, path),
    }
}

fn value_at_path(value: &Value, path: &str) -> Option<Value> {
    path.split('.')
        .try_fold(value, |current, segment| current.get(segment))
        .cloned()
}
