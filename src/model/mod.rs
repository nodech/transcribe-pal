use thiserror::Error;

mod line_parser;
mod manifest;

#[derive(Debug, Error)]
#[error("Failed to parse \"{0}\" manifest: {1}")]
pub struct LoadManifestError(&'static str, manifest::ModelManifestParseError);

pub fn load_manifests()
-> Result<Vec<manifest::ModelManifest>, LoadManifestError> {
    let raw_manifests =
        [("parakeet", include_str!("./data/parakeet.manifest"))];

    let mut manifests: Vec<manifest::ModelManifest> = vec![];

    for (name, data) in raw_manifests.into_iter() {
        let manifest: manifest::ModelManifest =
            data.parse().map_err(|e| LoadManifestError(name, e))?;
        manifests.push(manifest);
    }

    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifests() {
        load_manifests().unwrap();
    }
}
