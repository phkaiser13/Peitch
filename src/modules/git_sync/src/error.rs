use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Git operation failed: {0}")]
    GitError(#[from] git2::Error),

    #[error("Failed to parse YAML manifest: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Signature verification failed: {0}")]
    SignatureVerificationError(String),

    #[error("An unexpected error occurred: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
