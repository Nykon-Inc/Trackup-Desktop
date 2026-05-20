use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=NODE_ENV");
    println!("cargo:rerun-if-changed=../.env.production");
    println!("cargo:rerun-if-changed=../.env.staging");

    let node_env = env::var("NODE_ENV").unwrap_or_else(|_| "staging".to_string());
    
    // Find the correct .env file
    let env_file_name = if node_env == "production" {
        "../.env.production"
    } else {
        "../.env.staging"
    };

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let env_path = PathBuf::from(&manifest_dir).join(env_file_name);

    if let Ok(content) = fs::read_to_string(&env_path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "API_BASE_URL" {
                    // Remove quotes if present
                    let val = value.trim().trim_matches('"').trim_matches('\'');
                    println!("cargo:rustc-env=API_BASE_URL={}", val);
                }
            }
        }
    } else {
        println!("cargo:warning=Could not read env file at {:?}", env_path);
    }

    tauri_build::build()
}
