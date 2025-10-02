use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Heim {
    pub deploy: Deploy,
}

#[derive(Debug, Deserialize)]
pub struct Deploy {
    pub root_path: String,
    pub artifacts: Vec<Artifact>
}

#[derive(Debug, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub backup: bool,
    pub destination: String,
    pub excluded_files: Vec<String>,
    pub run_before: Option<Run>,
    pub run_after: Option<Run>
}

#[derive(Debug, Deserialize)]
pub struct Run {
    pub powershell: Option<String>
}


