use crate::manifest::InputSpec;
use anyhow::{bail, Result};
use rpassword::read_password;
use std::collections::HashMap;
use std::io::{self, Write};

pub fn collect_inputs(
    inputs: &[InputSpec],
    provided: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();

    for input in inputs {
        if let Some(value) = provided.get(&input.name) {
            values.insert(input.name.clone(), value.clone());
            continue;
        }

        loop {
            let value = if input.secret {
                prompt_secret(input)?
            } else {
                prompt_visible(input)?
            };

            let value = if value.is_empty() {
                input.default.clone().unwrap_or_default()
            } else {
                value
            };

            if input.required && value.is_empty() {
                println!("{} is required.", input.name);
                continue;
            }

            values.insert(input.name.clone(), value);
            break;
        }
    }

    for key in provided.keys() {
        if !inputs.iter().any(|input| input.name == *key) {
            bail!("provided value for unknown input '{key}'");
        }
    }

    Ok(values)
}

pub fn secret_values(inputs: &[InputSpec], values: &HashMap<String, String>) -> Vec<String> {
    inputs
        .iter()
        .filter(|input| input.secret)
        .filter_map(|input| values.get(&input.name).cloned())
        .filter(|value| !value.is_empty())
        .collect()
}

fn prompt_visible(input: &InputSpec) -> Result<String> {
    match &input.default {
        Some(default) => print!("{} [{}]: ", input.prompt, default),
        None => print!("{}: ", input.prompt),
    }
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_string())
}

fn prompt_secret(input: &InputSpec) -> Result<String> {
    match &input.default {
        Some(default) => print!("{} [{}]: ", input.prompt, default),
        None => print!("{}: ", input.prompt),
    }
    io::stdout().flush()?;
    Ok(read_password()?)
}
