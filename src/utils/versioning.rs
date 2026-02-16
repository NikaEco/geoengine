use regex::Regex;
use semver::Version;
use std::cmp::Ordering;
use crate::docker::client::DockerClient;

pub async fn get_latest_worker_version(worker_name: &str, client: &DockerClient) -> Option<String> {
    client.list_images(
            Some(&format!("geoengine-local/{}", worker_name)),
            true
        )
        .await
        .unwrap()
        .into_iter()
        .map(|i| {
            i.repo_tags
                .iter()
                .filter(|t| t.starts_with(&format!("geoengine-local/{}", worker_name)))
                .map(|v| {
                v.split(':').last().unwrap().to_string()
            }).collect::<Vec<String>>()
        })
        .flatten()
        .max()
}

pub async fn get_latest_worker_version_clientless(worker_name: &str) -> Option<String> {
    let client = DockerClient::new().await.unwrap();
    get_latest_worker_version(worker_name, &client).await
}

pub fn validate_version(version: &str) -> Result<(), String> {
    let valid = Regex::new(r"^(\d+\.)?(\d+\.)?(\*|\d+)$").unwrap();
    if !valid.is_match(version) {
        Err(format!("Invalid version '{}'. Version numbers should follow semantic versioning.", version))
    } else {
        Ok(())
    }
}

pub fn compare_versions(v1: &str, v2: &str) -> Result<Ordering, String> {
    validate_version(v1)?;
    validate_version(v2)?;
    let ver1 = match Version::parse(v1) {
        Ok(v) => v,
        Err(_) => return Err(format!("Invalid version '{}'. Please ensure your version number follows 'MAJOR.MINOR.PATCH'.", v1))
    };
    let ver2 = match Version::parse(v2) {
        Ok(v) => v,
        Err(_) => return Err(format!("Invalid version '{}'. Please ensure your version number follows 'MAJOR.MINOR.PATCH'.", v1))
    };
    Ok(ver1.cmp(&ver2))
}

/// Compare provided version with worker's built image version, throw an Error if version doesn't follow semantic versioning.
pub async fn compare_worker_version(worker_name: &str, version: &str, client: &DockerClient) -> Result<Ordering, String> {
    validate_version(version)?;
    let latest_version = get_latest_worker_version(worker_name, client).await;
    match latest_version {
        Some(latest) => {
            compare_versions(version, &latest)
        },
        None => Ok(Ordering::Greater)
    }
}