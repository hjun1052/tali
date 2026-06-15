use anyhow::{bail, Result};
use std::collections::HashMap;

pub fn interpolate(template: &str, values: &HashMap<String, String>) -> Result<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            bail!("unterminated interpolation expression");
        };
        let key = after_start[..end].trim();
        if key.is_empty() {
            bail!("empty interpolation expression");
        }
        let Some(value) = values.get(key) else {
            bail!("missing input value for '{key}'");
        };
        output.push_str(value);
        rest = &after_start[end + 2..];
    }

    output.push_str(rest);
    Ok(output)
}

pub fn mask_secrets(text: &str, secrets: &[String]) -> String {
    let mut masked = text.to_string();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        masked = masked.replace(secret, "********");
    }
    masked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_inputs() {
        let values = HashMap::from([
            ("name".to_string(), "tali".to_string()),
            ("mode".to_string(), "safe".to_string()),
        ]);
        assert_eq!(
            interpolate("hello {{ name }} in {{mode}}", &values).unwrap(),
            "hello tali in safe"
        );
    }

    #[test]
    fn masks_secret_values() {
        assert_eq!(
            mask_secrets("token abc123 appears twice abc123", &["abc123".to_string()]),
            "token ******** appears twice ********"
        );
    }
}
