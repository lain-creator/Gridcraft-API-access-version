//! Local, out-of-process LibreOffice import. No LibreOffice code is linked or bundled.
//! The original XLS is never passed to LibreOffice: only a private, bounded copy is.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(90);
// Network IP connections are denied; local Unix IPC needed by macOS is allowed.
#[cfg(target_os = "macos")]
const SANDBOX: &str = r#"(version 1)(allow default)(deny network-outbound (remote ip "*:*"))(deny network-inbound (local ip "*:*"))"#;
// Calc's Link setting: 0=always, 1=never, 2=on demand. A new, untrusted profile
// has no trusted macro sources. Settings affect this conversion only.
const PROFILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry">
<item oor:path="/org.openoffice.Office.Common/Security/Scripting">
<prop oor:name="DisableMacrosExecution" oor:op="fuse"><value>true</value></prop>
<prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop>
<prop oor:name="BlockUntrustedRefererLinks" oor:op="fuse"><value>true</value></prop>
<prop oor:name="DisableActiveContent" oor:op="fuse"><value>true</value></prop>
</item>
<item oor:path="/org.openoffice.Office.Calc/Content/Update">
<prop oor:name="Link" oor:op="fuse"><value>1</value></prop>
</item>
</oor:items>"#;

#[cfg(target_os = "macos")]
fn executable() -> Result<PathBuf, String> {
    let system = PathBuf::from("/Applications/LibreOffice.app/Contents/MacOS/soffice");
    let user = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications/LibreOffice.app/Contents/MacOS/soffice"));
    std::iter::once(system)
        .chain(user)
        .find(|p| p.is_file())
        .ok_or_else(|| "未找到 LibreOffice。请将官方 macOS 版安装到“应用程序”后重试；原 XLS 未更改。".into())
}

#[cfg(target_os = "windows")]
fn executable() -> Result<PathBuf, String> {
    ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .flat_map(|root| [root.join("LibreOffice/program/soffice.com"), root.join("Programs/LibreOffice/program/soffice.com")])
        .find(|p| p.is_file())
        .ok_or_else(|| "未找到 LibreOffice。请先安装官方 Windows 64 位版到默认目录，再打开 XLS；原文件未更改。".into())
}

fn converter_command(executable: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args(["-p", SANDBOX]).arg(executable);
        command
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(executable);
        command.creation_flags(0x08000000);
        command
    }
}

fn stop_converter(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("SystemRoot") {
        use std::os::windows::process::CommandExt;
        // Only stop this conversion's process tree, never another LibreOffice session.
        let _ = Command::new(PathBuf::from(root).join("System32/taskkill.exe"))
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("无法读取文件：{e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("请选择普通 XLS 文件。".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("兼容导入仅接受不超过 256 MB 的文件。".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("兼容导入仅接受不超过 256 MB 的文件。".into());
    }
    Ok(bytes)
}

pub fn convert(path: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    convert_with(path, &executable()?, cancelled, TIMEOUT)
}

fn convert_with(path: &str, executable: &Path, cancelled: &AtomicBool, timeout: Duration) -> Result<Vec<u8>, String> {
    let source = Path::new(path);
    if !source.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls")) {
        return Err("LibreOffice 兼容入口仅用于 .xls 文件。".into());
    }
    let bytes = read_bounded(source)?;
    // Reject renamed HTML, XLSX and random files before starting another parser.
    if !bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Err("这不是受支持的 Excel 97–2003 二进制 XLS 文件（可能是更老版本、网页表格或扩展名错误）。原文件未更改。".into());
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("已取消兼容导入。".into());
    }
    let temp = tempfile::Builder::new().prefix("gridcraft-xls-").tempdir().map_err(|e| e.to_string())?;
    let input = temp.path().join("input.xls");
    let output = temp.path().join("output");
    let profile = temp.path().join("profile");
    std::fs::create_dir_all(profile.join("user")).map_err(|e| e.to_string())?;
    std::fs::create_dir(&output).map_err(|e| e.to_string())?;
    std::fs::write(profile.join("user/registrymodifications.xcu"), PROFILE).map_err(|e| e.to_string())?;
    std::fs::write(&input, bytes).map_err(|e| e.to_string())?;
    let profile_url = url::Url::from_directory_path(&profile).map_err(|()| "无法建立临时配置路径。")?;
    let mut child = converter_command(executable)
        .arg(format!("-env:UserInstallation={profile_url}"))
        .args(["--headless", "--norestore", "--nodefault", "--nofirststartwizard", "--convert-to", "xlsx:Calc MS Excel 2007 XML", "--outdir"])
        .arg(&output)
        .arg(&input)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("无法启动本机 LibreOffice：{e}"))?;
    #[allow(clippy::disallowed_methods)] // Native desktop module, never wasm.
    let start = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) || start.elapsed() >= timeout {
            stop_converter(&mut child);
            return Err("兼容导入已取消或超过 90 秒。原 XLS 未更改；可用 LibreOffice 检查是否加密或损坏。".into());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(status)) => return Err(format!("LibreOffice 转换失败（{status}），原 XLS 未更改。")),
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                stop_converter(&mut child);
                return Err(format!("无法等待 LibreOffice：{e}"));
            }
        }
    }
    let converted = read_bounded(&output.join("input.xlsx"))
        .map_err(|e| format!("LibreOffice 未生成可用的 XLSX：{e}。请检查原文件是否加密、损坏或需要交互；原文件未更改。"))?;
    if !converted.starts_with(b"PK\x03\x04") {
        return Err("转换结果不是有效的 XLSX 容器；未导入，原文件未更改。".into());
    }
    Ok(converted)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // This module tests native macOS processes and timing only.
mod tests {
    use super::*;

    #[test]
    fn invalid_files_do_not_launch_converter_or_modify_source() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("中文 ' quoted.XLS");
        std::fs::write(&path, b"not XLS")?;
        let path_string = path.to_string_lossy();
        let result = convert_with(&path_string, Path::new("/not/an/executable"), &AtomicBool::new(false), TIMEOUT);
        assert!(result.is_err_and(|e| e.contains("二进制 XLS")));
        assert_eq!(std::fs::read(&path)?, b"not XLS");
        Ok(())
    }

    #[test]
    fn cancellation_does_not_launch_converter() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.xls");
        std::fs::write(&path, [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])?;
        let result = convert_with(&path.to_string_lossy(), Path::new("/not/an/executable"), &AtomicBool::new(true), TIMEOUT);
        assert!(result.is_err_and(|e| e.contains("已取消")));
        Ok(())
    }

    #[test]
    fn oversized_input_is_rejected_before_allocation() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("large.xls");
        std::fs::File::create(&path)?.set_len(MAX_BYTES + 1)?;
        assert!(read_bounded(&path).is_err_and(|e| e.contains("256 MB")));
        Ok(())
    }

    #[test]
    fn security_profile_disables_active_content_and_link_updates() {
        assert!(PROFILE.contains("DisableMacrosExecution\" oor:op=\"fuse\"><value>true"));
        assert!(PROFILE.contains("DisableActiveContent\" oor:op=\"fuse\"><value>true"));
        assert!(PROFILE.contains("Link\" oor:op=\"fuse\"><value>1"));
        #[cfg(target_os = "macos")]
        assert!(SANDBOX.contains("deny network-outbound"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn failed_or_stalled_converter_cannot_silently_fall_back() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.xls");
        let original = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        std::fs::write(&path, original)?;
        let fake = dir.path().join("converter");
        std::fs::write(&fake, "#!/bin/sh\nexit 0\n")?;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700))?;
        let result = convert_with(&path.to_string_lossy(), &fake, &AtomicBool::new(false), TIMEOUT);
        assert!(result.is_err_and(|e| e.contains("未生成")));
        std::fs::write(&fake, "#!/bin/sh\nexec /bin/sleep 20\n")?;
        let start = Instant::now();
        let result = convert_with(&path.to_string_lossy(), &fake, &AtomicBool::new(false), Duration::from_millis(300));
        assert!(result.is_err_and(|e| e.contains("超过")));
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(std::fs::read(&path)?, original);
        Ok(())
    }

    #[test]
    #[ignore = "requires an installed official LibreOffice for a local integration test"]
    fn real_libreoffice_roundtrip_with_synthetic_xls() -> Result<(), Box<dyn std::error::Error>> {
        use serde_json::json;
        let dir = tempfile::tempdir()?;
        let mut session = gridcraft_engine::Session::new();
        session.new_workbook();
        session.run("cell.set", json!({"cell":"A1", "input":"5"}))?;
        session.run("cell.set", json!({"cell":"B1", "input":"=A1*2"}))?;
        session.run("cell.set", json!({"cell":"A2", "input":"中文测试"}))?;
        session.run("home.bold", json!({"range":"A1", "on":true}))?;
        session.run("home.mergeCells", json!({"range":"A2:C2"}))?;
        session.run("file.save", json!({"path":dir.path().join("synthetic.xlsx")}))?;
        let profile = dir.path().join("profile");
        std::fs::create_dir_all(profile.join("user"))?;
        std::fs::write(profile.join("user/registrymodifications.xcu"), PROFILE)?;
        let profile_url = url::Url::from_directory_path(&profile).map_err(|()| "invalid profile")?;
        let mut child = converter_command(&executable()?)
            .arg(format!("-env:UserInstallation={profile_url}"))
            .args(["--headless", "--norestore", "--convert-to", "xls:MS Excel 97", "--outdir"])
            .arg(dir.path())
            .arg(dir.path().join("synthetic.xlsx"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                assert!(status.success());
                break;
            }
            if start.elapsed() > TIMEOUT {
                child.kill()?;
                let _ = child.wait();
                return Err("fixture conversion timeout".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let path = dir.path().join("中文 ' quoted.XLS");
        std::fs::rename(dir.path().join("synthetic.xls"), &path)?;
        let original = std::fs::read(&path)?;
        let converted = convert(&path.to_string_lossy(), &AtomicBool::new(false))?;
        assert_eq!(std::fs::read(&path)?, original);
        let mut imported = gridcraft_engine::Session::new();
        imported.run("file.open", json!({"name":"import.xlsx", "base64":gridcraft_engine::io::base64_encode(&converted)}))?;
        let a = imported.run("cell.get", json!({"cell":"A1"}))?;
        let b = imported.run("cell.get", json!({"cell":"B1"}))?;
        let c = imported.run("cell.get", json!({"cell":"A2"}))?;
        assert_eq!(a["value"], 5.0);
        assert_eq!(a["style"]["font"]["bold"], true);
        assert_eq!(b["value"], 10.0);
        assert_eq!(b["formula"], "=A1*2");
        assert_eq!(c["value"], "中文测试");
        assert_eq!(c["merge"], "A2:C2");
        Ok(())
    }
}
