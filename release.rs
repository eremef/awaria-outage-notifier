use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <version>", args[0]);
        eprintln!("Example: {} 1.0.8", args[0]);
        std::process::exit(1);
    }

    let new_version = &args[1];

    update_cargo_toml(new_version);
    update_package_json(new_version);
    update_tauri_conf(new_version);

    run_command("npm", &["install"], ".");
    run_command("cargo", &["update"], "src-tauri");
    run_command("git", &["add", "."], ".");
    run_command("git", &["commit", "-m", &("release: v".to_string()+new_version)], ".");
    run_command("git", &["push", "origin", "HEAD"], ".");
    run_command("git", &["tag", "-d", &("v".to_string()+new_version)], ".");
    run_command("git", &["push", "origin", "--delete", &("v".to_string()+new_version)], ".");
    run_command("git", &["tag", &("v".to_string()+new_version)], ".");
    run_command("git", &["push", "origin", &("v".to_string()+new_version)], ".");

    println!("Updated version to {}", new_version);
}

fn update_cargo_toml(version: &str) {
    let cargo_path = Path::new("src-tauri").join("Cargo.toml");

    if !cargo_path.exists() {
        eprintln!("Warning: Cargo.toml not found at {}", cargo_path.display());
        return;
    }

    let content = fs::read_to_string(&cargo_path).expect("Failed to read Cargo.toml");

    let new_content = content
        .lines()
        .map(|line| {
            if line.trim().starts_with("version = ") {
                format!("version = \"{}\"", version)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&cargo_path, new_content).expect("Failed to write Cargo.toml");

    println!("Updated src-tauri/Cargo.toml");
}

fn update_package_json(version: &str) {
    let package_path = Path::new("package.json");

    if !package_path.exists() {
        eprintln!("Warning: package.json not found");
        return;
    }

    let content = fs::read_to_string(package_path).expect("Failed to read package.json");

    let new_content = content
        .lines()
        .map(|line| {
            if line.trim().starts_with("\"version\":") {
                format!("  \"version\": \"{}\",", version)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(package_path, new_content).expect("Failed to write package.json");

    println!("Updated package.json");
}

fn update_tauri_conf(version: &str) {
    let tauri_version: String = version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    let tauri_path = Path::new("src-tauri").join("tauri.conf.json");

    if !tauri_path.exists() {
        eprintln!("Warning: tauri.conf.json not found");
        return;
    }

    let content = fs::read_to_string(&tauri_path).expect("Failed to read tauri.conf.json");

    let new_content = content
        .lines()
        .map(|line| {
            if line.contains("\"version\"") && !line.contains("$schema") {
                let indent = line
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>();
                format!("{}  \"version\": \"{}\",", indent, tauri_version)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    fs::write(&tauri_path, new_content).expect("Failed to write tauri.conf.json");

    println!("Updated src-tauri/tauri.conf.json");
}

fn run_command(program: &str, args: &[&str], dir: &str) {
    let status = Command::new(program).args(args).current_dir(dir).status();

    match status {
        Ok(s) if s.success() => println!("Ran {} {}", program, args.join(" ")),
        Ok(s) => eprintln!("Command {} failed with exit code: {:?}", program, s.code()),
        Err(e) => eprintln!("Failed to run {}: {}", program, e),
    }
}
