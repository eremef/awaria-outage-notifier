use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    tauri_build::build();

    if env::var("CARGO_CFG_TARGET_OS").is_ok_and(|target_os| target_os == "windows")
        && env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|target_env| target_env == "msvc")
    {
        let manifest = PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR")
                .expect("Cargo sets CARGO_MANIFEST_DIR for build scripts"),
        )
        .join("windows-test.manifest");
        let out_dir =
            PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts"));
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH")
            .expect("Cargo sets CARGO_CFG_TARGET_ARCH for build scripts");
        let (resource_arch, machine) = match target_arch.as_str() {
            "x86_64" => ("x64", "X64"),
            "x86" => ("x86", "X86"),
            "aarch64" => ("arm64", "ARM64"),
            _ => panic!("unsupported Windows MSVC test target architecture: {target_arch}"),
        };

        let rc = find_windows_sdk_tool("rc.exe", resource_arch)
            .expect("Windows SDK rc.exe is required to embed the test manifest");
        let cvtres = find_msvc_tool("cvtres.exe", resource_arch)
            .expect("MSVC cvtres.exe is required to embed the test manifest");
        let lib = find_msvc_tool("lib.exe", resource_arch)
            .expect("MSVC lib.exe is required to embed the test manifest");

        let manifest_for_rc = manifest.to_string_lossy().replace('\\', "/");
        let resource_script = out_dir.join("windows-test.rc");
        let resource_file = out_dir.join("windows-test.res");
        let object_file = out_dir.join("windows-test.obj");
        let library_file = out_dir.join("awaria_windows_test_manifest.lib");
        fs::write(&resource_script, format!("1 24 \"{manifest_for_rc}\"\n"))
            .expect("failed to write the Windows test manifest resource script");

        let mut rc_command = Command::new(&rc);
        rc_command
            .args(["/nologo", "/fo"])
            .arg(&resource_file)
            .arg(&resource_script);
        run_tool(&mut rc_command);

        let mut cvtres_command = Command::new(&cvtres);
        cvtres_command
            .arg(format!("/MACHINE:{machine}"))
            .arg(format!("/OUT:{}", object_file.display()))
            .arg(&resource_file);
        run_tool(&mut cvtres_command);

        let mut lib_command = Command::new(&lib);
        lib_command
            .arg("/NOLOGO")
            .arg(format!("/OUT:{}", library_file.display()))
            .arg(&object_file);
        run_tool(&mut lib_command);

        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}

fn run_tool(command: &mut Command) {
    let tool = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to run {tool}: {error}"));
    assert!(status.success(), "{tool} failed with {status}");
}

fn find_windows_sdk_tool(name: &str, target_arch: &str) -> Option<PathBuf> {
    if let Some(sdk_dir) = env::var_os("WindowsSdkDir") {
        let sdk_dir = PathBuf::from(sdk_dir);
        if let Some(version) = env::var_os("WindowsSDKVersion") {
            let path = sdk_dir
                .join("bin")
                .join(version.to_string_lossy().trim_end_matches(['\\', '/']))
                .join(target_arch)
                .join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    let program_files = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let bin_dir = program_files.join("Windows Kits").join("10").join("bin");
    let mut versions = fs::read_dir(&bin_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    versions.sort_by_key(|entry| entry.file_name());

    versions.into_iter().rev().find_map(|version| {
        let path = version.path().join(target_arch).join(name);
        path.is_file().then_some(path)
    })
}

fn find_msvc_tool(name: &str, target_arch: &str) -> Option<PathBuf> {
    let tools_dir = env::var_os("VCToolsInstallDir")
        .map(PathBuf::from)
        .or_else(find_msvc_tools_dir)?;
    let host_arch = match env::consts::ARCH {
        "x86_64" => "Hostx64",
        "x86" => "Hostx86",
        _ => return None,
    };
    let path = tools_dir
        .join("bin")
        .join(host_arch)
        .join(target_arch)
        .join(name);
    path.is_file().then_some(path)
}

fn find_msvc_tools_dir() -> Option<PathBuf> {
    let program_files = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let vswhere = program_files
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let installation = String::from_utf8(output.stdout).ok()?;
    let versions_dir = PathBuf::from(installation.trim())
        .join("VC")
        .join("Tools")
        .join("MSVC");
    let mut versions = fs::read_dir(versions_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    versions.sort_by_key(|entry| entry.file_name());
    versions.last().map(|entry| entry.path())
}
