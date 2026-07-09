pub const HOST_CAPABILITIES: &[&str] = &[
    "unrestricted_http",
    "browser_payload",
    "rhai_scripting",
    "scoped_cache",
];

pub fn check_min_kani_version(min_version: Option<&str>, host_version: &str) -> Result<(), String> {
    let Some(min_version) = min_version else {
        return Ok(());
    };
    let required = semver::Version::parse(min_version)
        .map_err(|e| format!("invalid min_kani_version '{min_version}': {e}"))?;
    let host = semver::Version::parse(host_version)
        .map_err(|e| format!("invalid host version '{host_version}': {e}"))?;
    if host < required {
        return Err(format!(
            "extension requires kani >= {min_version}, but this host is running {host_version}"
        ));
    }
    Ok(())
}

pub fn check_required_capabilities(required: &[String]) -> Result<(), String> {
    for cap in required {
        if !HOST_CAPABILITIES.contains(&cap.as_str()) {
            return Err(format!("extension requires unsupported capability '{cap}'"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn min_version_none_always_passes() {
        assert!(check_min_kani_version(None, "0.1.0").is_ok());
    }

    #[test]
    fn min_version_satisfied_passes() {
        assert!(check_min_kani_version(Some("0.1.0"), "0.1.0").is_ok());
        assert!(check_min_kani_version(Some("0.1.0"), "1.0.0").is_ok());
    }

    #[test]
    fn min_version_unsatisfied_fails() {
        let err = check_min_kani_version(Some("0.5.0"), "0.1.0").unwrap_err();
        assert!(err.contains("0.5.0"));
        assert!(err.contains("0.1.0"));
    }

    #[test]
    fn min_version_invalid_semver_fails() {
        assert!(check_min_kani_version(Some("not-a-version"), "0.1.0").is_err());
    }

    #[test]
    fn capabilities_empty_passes() {
        assert!(check_required_capabilities(&[]).is_ok());
    }

    #[test]
    fn capabilities_supported_passes() {
        assert!(check_required_capabilities(&["unrestricted_http".to_string()]).is_ok());
    }

    #[test]
    fn capabilities_unsupported_fails() {
        let err = check_required_capabilities(&["does_not_exist".to_string()]).unwrap_err();
        assert!(err.contains("does_not_exist"));
    }
}
