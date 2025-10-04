use tokio::fs::File;
use tracing::error;

use serde::Deserialize;

use crate::error::{ErrorKind, HeimError};

#[derive(Debug, Deserialize)]
pub struct Heim {
    pub deploy: Deploy,
}

#[derive(Debug, Deserialize)]
pub struct Deploy {
    pub root_path: String,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub backup: bool,
    pub destination: String,
    pub excluded_files: Option<Vec<String>>,
    pub run_before: Option<Run>,
    pub run_after: Option<Run>,
}

#[derive(Debug, Deserialize)]
pub struct Run {
    pub powershell: Option<String>,
}

pub async fn load_heim(path: &str) -> Result<Heim, HeimError> {
    let file = tokio::fs::read_to_string(path).await.map_err(|e| {
        error!("Failed to read Heim.json: {}", e);
        HeimError {
            kind: ErrorKind::ArtifactError,
            message: "Failed to read Heim.json.",
        }
    })?;

    let heim: Heim = serde_json::from_str(&file).map_err(|e| {
        error!("Failed to parse Heim.json: {}", e);
        HeimError {
            kind: ErrorKind::ArtifactError,
            message: "Failed to read Heim.json.",
        }
    })?;

    Ok(heim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_heim_example() {
        let data = r#"
        {
          "deploy": {
            "root_path": "D:\\projects\\artifacts",
            "artifacts": [
              {
                "id": "example-service",
                "kind": "java",
                "backup": true,
                "destination": "example-service",
                "excluded_files": [
                  "up.ps1"
                ],
                "run_after": {
                  "powershell": "Start-Process powershell -ArgumentList 'D:\\projects\\artifacts\\example-service\\up.ps1'"
                }
              },
              {
                "id": "example-client",
                "kind": "angular",
                "backup": true,
                "destination": "example-client\\example"
              }
            ]
          }
        }
        "#;

        let heim: Heim = serde_json::from_str(data).expect("Deserialization failed");

        assert_eq!(heim.deploy.root_path, "D:\\projects\\artifacts");
        assert_eq!(heim.deploy.artifacts.len(), 2);

        let service = &heim.deploy.artifacts[0];
        assert_eq!(service.id, "example-service");
        assert_eq!(service.kind, "java");
        assert_eq!(service.backup, true);
        assert_eq!(service.destination, "example-service");
        assert_eq!(service.excluded_files.as_ref().unwrap(), &vec!["up.ps1"]);
        assert!(service.run_before.is_none());
        assert!(service.run_after.is_some());
        assert_eq!(
            service
                .run_after
                .as_ref()
                .unwrap()
                .powershell
                .as_ref()
                .unwrap(),
            "Start-Process powershell -ArgumentList 'D:\\projects\\artifacts\\example-service\\up.ps1'"
        );

        let client = &heim.deploy.artifacts[1];
        assert_eq!(client.id, "example-client");
        assert_eq!(client.kind, "angular");
        assert_eq!(client.backup, true);
        assert_eq!(client.destination, "example-client\\example");
        assert!(client.excluded_files.is_none());
        assert!(client.run_before.is_none());
        assert!(client.run_after.is_none());
    }
}
