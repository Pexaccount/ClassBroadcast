#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod cloud;
mod commands;
mod config;
mod lan;
mod overlay;
mod protocol;

use config::Mode;

fn main() {
    let build_role = if cfg!(feature = "teacher-ui") {
        Some(Mode::Teacher)
    } else {
        None
    };
    // 尝试放行 Windows 防火墙（需管理员权限；失败则静默忽略，可手动放行）
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    "name=ClassBroadcast",
                    "dir=in",
                    "action=allow",
                    &format!("program={}", exe.display()),
                    "enable=yes",
                ])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .output();
        }
    }
    commands::run(build_role);
}
