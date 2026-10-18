use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    Validation(String),
    Network(String),
    Format(String),
    ModelNotReady(String),
    PythonEnvironment(String),
    OperationConflict,
    Download(String),
    Cancelled,
    General(String),
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }

    pub fn format(message: impl Into<String>) -> Self {
        Self::Format(message.into())
    }

    pub fn model_not_ready(model: impl Into<String>) -> Self {
        Self::ModelNotReady(model.into())
    }

    pub fn python(message: impl Into<String>) -> Self {
        Self::PythonEnvironment(message.into())
    }

    pub fn download(message: impl Into<String>) -> Self {
        Self::Download(message.into())
    }

    pub fn general(message: impl Into<String>) -> Self {
        Self::General(message.into())
    }

    pub fn message(&self) -> String {
        match self {
            Self::Validation(value) => value.clone(),
            Self::Network(value) => value.clone(),
            Self::Format(value) => value.clone(),
            Self::ModelNotReady(model) => format!("Model {model} is not ready."),
            Self::PythonEnvironment(value) => value.clone(),
            Self::OperationConflict => "Another task is already running.".to_string(),
            Self::Download(value) => value.clone(),
            Self::Cancelled => "Operation cancelled.".to_string(),
            Self::General(value) => value.clone(),
        }
    }

    pub fn is_validation(&self) -> bool {
        matches!(self, Self::Validation(_))
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::General(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        Self::Format(error.to_string())
    }
}
