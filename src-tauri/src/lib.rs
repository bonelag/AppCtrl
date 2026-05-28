use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    menu::{Menu, MenuItem, Submenu},
    AppHandle, Emitter, Manager,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

struct ProcessManager {
    processes: Mutex<HashMap<String, Child>>,
}

impl ProcessManager {
    fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }
}

#[tauri::command]
async fn start_app(
    app_handle: AppHandle,
    app_id: String,
    path: String,
    _app_type: String,
    working_dir: String,
    _args: String,
    env_vars: String,
) -> Result<(), String> {
    let state = app_handle.state::<ProcessManager>();
    
    {
        let processes = state.processes.lock().unwrap();
        if processes.contains_key(&app_id) {
            return Err("App is already running".to_string());
        }
    }
    
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd.exe");
        // Run chcp 65001 (UTF-8) before the actual command
        let full_cmd = format!("chcp 65001 >nul && {}", path);
        c.args(["/C", &full_cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", &path]);
        c
    };
    
    if !working_dir.is_empty() {
        cmd.current_dir(&working_dir);
        let _ = app_handle.emit("app-output", serde_json::json!({
            "appId": &app_id,
            "line": format!("📁 Working dir: {}", working_dir)
        }));
    } else if let Some(parent) = std::path::Path::new(&path).parent() {
        if parent.exists() && !parent.as_os_str().is_empty() {
            cmd.current_dir(parent);
        }
    }
    
    // Set UTF-8 encoding for proper Unicode support
    cmd.env("PYTHONIOENCODING", "utf-8");
    cmd.env("PYTHONUTF8", "1");
    cmd.env("CHCP", "65001");
    
    if !env_vars.is_empty() {
        for line in env_vars.lines() {
            let line = line.trim();
            if !line.is_empty() {
                if let Some((key, value)) = line.split_once('=') {
                    cmd.env(key.trim(), value.trim());
                }
            }
        }
    }
    
    cmd.stdout(Stdio::piped())
       .stderr(Stdio::piped())
       .stdin(Stdio::null());
    
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    
    let result = cmd.spawn();
    
    let mut child = match result {
        Ok(c) => c,
        Err(e) => {
            let _ = app_handle.emit("app-output", serde_json::json!({
                "appId": &app_id,
                "line": format!("❌ Failed to start: {}", e)
            }));
            return Err(format!("Failed to start: {}", e));
        }
    };
    
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    
    let _ = app_handle.emit("app-output", serde_json::json!({
        "appId": &app_id,
        "line": format!("✓ Started: {}", path)
    }));
    
    {
        let mut processes = state.processes.lock().unwrap();
        processes.insert(app_id.clone(), child);
    }
    
    if let Some(stdout) = stdout {
        let app_handle_clone = app_handle.clone();
        let app_id_clone = app_id.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let _ = app_handle_clone.emit("app-output", serde_json::json!({
                    "appId": &app_id_clone,
                    "line": line
                }));
            }
        });
    }
    
    if let Some(stderr) = stderr {
        let app_handle_clone = app_handle.clone();
        let app_id_clone = app_id.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                let _ = app_handle_clone.emit("app-output", serde_json::json!({
                    "appId": &app_id_clone,
                    "line": format!("[stderr] {}", line)
                }));
            }
        });
    }
    
    let app_handle_exit = app_handle.clone();
    let app_id_exit = app_id.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let state = app_handle_exit.state::<ProcessManager>();
            let mut processes = state.processes.lock().unwrap();
            
            if let Some(child) = processes.get_mut(&app_id_exit) {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code = status.code().unwrap_or(-1);
                        let msg = if code == 0 {
                            "✓ Process exited successfully".to_string()
                        } else {
                            format!("⚠ Process exited with code: {}", code)
                        };
                        let _ = app_handle_exit.emit("app-output", serde_json::json!({
                            "appId": &app_id_exit,
                            "line": msg
                        }));
                        let _ = app_handle_exit.emit("app-stopped", serde_json::json!({
                            "appId": &app_id_exit
                        }));
                        processes.remove(&app_id_exit);
                        break;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        processes.remove(&app_id_exit);
                        break;
                    }
                }
            } else {
                break;
            }
        }
    });
    
    Ok(())
}

#[tauri::command]
async fn stop_app(app_handle: AppHandle, app_id: String, exe_path: Option<String>) -> Result<(), String> {
    let state = app_handle.state::<ProcessManager>();
    
    let mut processes = state.processes.lock().unwrap();
    if let Some(mut child) = processes.remove(&app_id) {
        #[cfg(windows)]
        {
            let pid = child.id();
            let _ = Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .creation_flags(0x08000000)
                .output();
        }
        
        let _ = child.kill();
        let _ = app_handle.emit("app-output", serde_json::json!({
            "appId": &app_id,
            "line": "■ Process stopped by user"
        }));
        let _ = app_handle.emit("app-stopped", serde_json::json!({
            "appId": &app_id
        }));
        Ok(())
    } else {
        // Try to kill by executable name if provided
        if let Some(path) = exe_path {
            let path = std::path::Path::new(&path);
            if let Some(file_name) = path.file_name() {
                if let Some(name_str) = file_name.to_str() {
                    #[cfg(windows)]
                    {
                        let output = Command::new("taskkill")
                            .args(["/F", "/IM", name_str])
                            .creation_flags(0x08000000)
                            .output()
                            .map_err(|e| e.to_string())?;
                            
                        if output.status.success() {
                             let _ = app_handle.emit("app-output", serde_json::json!({
                                "appId": &app_id,
                                "line": format!("■ External process {} stopped", name_str)
                            }));
                            let _ = app_handle.emit("app-stopped", serde_json::json!({
                                "appId": &app_id
                            }));
                            return Ok(());
                        }
                    }
                }
            }
        }
        Err("App is not running".to_string())
    }
}

#[tauri::command]
fn check_process_running(exe_path: String) -> bool {
    let path = std::path::Path::new(&exe_path);
    if let Some(file_name) = path.file_name() {
        if let Some(name_str) = file_name.to_str() {
            #[cfg(windows)]
            {
                // Use tasklist to check if process exists
                // /FI "IMAGENAME eq name.exe" /NH (No Header)
                let output = Command::new("tasklist")
                    .args(["/FI", &format!("IMAGENAME eq {}", name_str), "/NH"])
                    .creation_flags(0x08000000)
                    .output();
                    
                if let Ok(out) = output {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    // If process found, it will list it. If not, it says "INFO: No tasks are running..."
                    return stdout.to_lowercase().contains(&name_str.to_lowercase());
                }
            }
        }
    }
    false
}

#[tauri::command]
fn is_app_running(app_handle: AppHandle, app_id: String) -> bool {
    let state = app_handle.state::<ProcessManager>();
    let processes = state.processes.lock().unwrap();
    processes.contains_key(&app_id)
}

// Extract icon from EXE file and return as base64 data URL
#[cfg(windows)]
#[tauri::command]
fn extract_exe_icon(exe_path: String) -> Result<String, String> {
    use std::ptr::null_mut;
    use winapi::um::shellapi::ExtractIconExW;
    use winapi::um::winuser::{GetIconInfo, ICONINFO};
    use winapi::um::wingdi::{
        GetDIBits, CreateCompatibleDC, DeleteDC, GetObjectW, BITMAP, BITMAPINFO, 
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject,
    };
    use winapi::shared::windef::HICON;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    
    // Convert path to wide string
    let wide_path: Vec<u16> = OsStr::new(&exe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    // Manually declare PrivateExtractIconsW as it might be missing in winapi
    #[link(name = "user32")]
    extern "system" {
        fn PrivateExtractIconsW(
            szFileName: winapi::um::winnt::LPCWSTR,
            nIconIndex: i32,
            cxIcon: i32,
            cyIcon: i32,
            phicon: *mut HICON,
            piconid: *mut u32,
            nIcons: u32,
            flags: u32,
        ) -> u32;
    }

    unsafe {
        // Try to extract a large icon (256x256)
        let mut hicon: HICON = null_mut();
        let mut icon_id: u32 = 0;
        
        let count = PrivateExtractIconsW(
            wide_path.as_ptr(),
            0,
            256, // Width
            256, // Height
            &mut hicon,
            &mut icon_id,
            1,
            0,
        );
        
        if count == 0 || hicon.is_null() {
            // Fallback to ExtractIconExW if PrivateExtractIconsW fails
             let count_ex = ExtractIconExW(
                wide_path.as_ptr(),
                0,
                &mut hicon,
                null_mut(),
                1,
            );
            if count_ex == 0 || hicon.is_null() {
                return Err("No icon found in EXE".to_string());
            }
        }
        
        // Get icon info
        let mut icon_info: ICONINFO = std::mem::zeroed();
        if GetIconInfo(hicon, &mut icon_info) == 0 {
            return Err("Failed to get icon info".to_string());
        }
        
        // Get bitmap info
        let mut bmp: BITMAP = std::mem::zeroed();
        GetObjectW(
            icon_info.hbmColor as _,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bmp as *mut _ as *mut _,
        );
        
        let width = bmp.bmWidth as usize;
        let height = bmp.bmHeight as usize;
        
        if width == 0 || height == 0 {
            DeleteObject(icon_info.hbmColor as _);
            DeleteObject(icon_info.hbmMask as _);
            winapi::um::winuser::DestroyIcon(hicon);
            return Err("Invalid icon dimensions".to_string());
        }
        
        // Create DC
        let hdc = CreateCompatibleDC(null_mut());
        
        // Setup bitmap info
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = width as i32;
        bmi.bmiHeader.biHeight = -(height as i32); // Top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;
        
        // Get pixel data
        let mut pixels: Vec<u8> = vec![0; width * height * 4];
        GetDIBits(
            hdc,
            icon_info.hbmColor,
            0,
            height as u32,
            pixels.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        
        // Convert BGRA to RGBA
        for chunk in pixels.chunks_mut(4) {
            chunk.swap(0, 2); // Swap B and R
        }
        
        // Cleanup
        DeleteDC(hdc);
        DeleteObject(icon_info.hbmColor as _);
        DeleteObject(icon_info.hbmMask as _);
        winapi::um::winuser::DestroyIcon(hicon);
        
        // Create PNG image
        let img = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
            .ok_or("Failed to create image")?;
        
        let mut png_data: Vec<u8> = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode PNG: {}", e))?;
        
        // Encode to base64 data URL
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
        Ok(format!("data:image/png;base64,{}", b64))
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn extract_exe_icon(_exe_path: String) -> Result<String, String> {
    Err("Icon extraction only supported on Windows".to_string())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

struct AppSettings {
    minimize_to_tray: Mutex<bool>,
}

#[tauri::command]
fn set_minimize_to_tray(app_handle: AppHandle, minimize: bool) {
    let state = app_handle.state::<AppSettings>();
    *state.minimize_to_tray.lock().unwrap() = minimize;
}

#[tauri::command]
fn get_config_path() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
    path.pop(); // Remove exe name
    path.push("config.json");
    path
}

#[tauri::command]
fn load_config() -> Result<String, String> {
    let path = get_config_path();
    if path.exists() {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    } else {
        Ok("{}".to_string()) // Return empty JSON object if not found
    }
}

#[tauri::command]
fn save_config(config: String) -> Result<(), String> {
    let path = get_config_path();
    std::fs::write(path, config).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
struct PortInfo {
    port: u16,
    pid: u32,
    name: String,
    protocol: String,
}

#[tauri::command]
async fn get_listening_ports() -> Result<Vec<PortInfo>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        
        // 1. Get all processes (PID -> Name)
        let output = Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to run tasklist: {}", e))?;
            
        let tasklist_out = String::from_utf8_lossy(&output.stdout);
        let mut pid_map = HashMap::new();
        
        for line in tasklist_out.lines() {
            // CSV format: "Name","PID",...
            let parts: Vec<&str> = line.split("\",\"").collect();
            if parts.len() >= 2 {
                let name = parts[0].trim_matches('"').to_string();
                let pid_str = parts[1].trim_matches('"');
                if let Ok(pid) = pid_str.parse::<u32>() {
                    pid_map.insert(pid, name);
                }
            }
        }
        
        // 2. Get listening ports
        let output = Command::new("netstat")
            .args(["-ano"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to run netstat: {}", e))?;
            
        let netstat_out = String::from_utf8_lossy(&output.stdout);
        let mut ports = Vec::new();
        
        for line in netstat_out.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Expected: Proto, Local Address, Foreign Address, State, PID
            // TCP 0.0.0.0:80 0.0.0.0:0 LISTENING 1234
            // UDP 0.0.0.0:123 *:* 1234
            
            if parts.len() >= 5 && parts[0] == "TCP" && parts[3] == "LISTENING" {
                let local_addr = parts[1];
                let pid_str = parts[4];
                
                if let Some(port_str) = local_addr.split(':').last() {
                    if let (Ok(port), Ok(pid)) = (port_str.parse::<u16>(), pid_str.parse::<u32>()) {
                        let name = pid_map.get(&pid).cloned().unwrap_or_else(|| "Unknown".to_string());
                        ports.push(PortInfo {
                            port,
                            pid,
                            name,
                            protocol: "TCP".to_string(),
                        });
                    }
                }
            } else if parts.len() >= 4 && parts[0] == "UDP" {
                // UDP doesn't have "State" column usually, PID is at index 3
                let local_addr = parts[1];
                let pid_str = parts[3];
                 if let Some(port_str) = local_addr.split(':').last() {
                    if let (Ok(port), Ok(pid)) = (port_str.parse::<u16>(), pid_str.parse::<u32>()) {
                        let name = pid_map.get(&pid).cloned().unwrap_or_else(|| "Unknown".to_string());
                        ports.push(PortInfo {
                            port,
                            pid,
                            name,
                            protocol: "UDP".to_string(),
                        });
                    }
                }
            }
        }
        
        // Sort by port
        ports.sort_by_key(|p| p.port);
        // Deduplicate (sometimes netstat shows multiple lines for same socket)
        ports.dedup_by(|a, b| a.port == b.port && a.pid == b.pid && a.protocol == b.protocol);
        
        Ok(ports)
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on non-Windows yet".to_string())
    }
}


#[tauri::command]
async fn kill_process_by_pid(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| e.to_string())?;
            
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on non-Windows yet".to_string())
    }
}

#[tauri::command]
async fn kill_process_by_name(name: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("taskkill")
            .args(["/F", "/IM", &name])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| e.to_string())?;
            
        if output.status.success() {
            Ok(())
        } else {
            // Check if error is "The process ... not found" (which means success effectively)
            let err = String::from_utf8_lossy(&output.stderr).to_string();
            if err.contains("not found") {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on non-Windows yet".to_string())
    }
}

#[derive(serde::Serialize)]
struct ProcessInfo {
    pid: u32,
    name: String,
    memory: String,
}

#[tauri::command]
async fn get_processes() -> Result<Vec<ProcessInfo>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to run tasklist: {}", e))?;

        let tasklist_out = String::from_utf8_lossy(&output.stdout);
        let mut processes = Vec::new();
        
        let system_processes = [
            "System Idle Process", "System", "Registry", "smss.exe", "csrss.exe", 
            "wininit.exe", "services.exe", "lsass.exe", "svchost.exe", "fontdrvhost.exe", 
            "dwm.exe", "winlogon.exe", "spoolsv.exe", "Memory Compression", "taskhostw.exe",
            "RuntimeBroker.exe", "SearchUI.exe", "ShellExperienceHost.exe", "ApplicationFrameHost.exe",
            "ctfmon.exe", "conhost.exe", "dllhost.exe", "sihost.exe", "SearchApp.exe",
            "StartMenuExperienceHost.exe", "TextInputHost.exe", "SecurityHealthService.exe",
            "NisSrv.exe", "MsMpEng.exe", "audiodg.exe"
        ];

        for line in tasklist_out.lines() {
            // "Name","PID","Session Name","Session#","Mem Usage"
            let parts: Vec<&str> = line.split("\",\"").collect();
            if parts.len() >= 5 {
                let name = parts[0].trim_matches('"').to_string();
                
                // Filter system processes
                if system_processes.iter().any(|&s| s.eq_ignore_ascii_case(&name)) {
                    continue;
                }

                let pid_str = parts[1].trim_matches('"');
                let mem_str = parts[4].trim_matches('"'); // e.g. "12,345 K"
                
                if let Ok(pid) = pid_str.parse::<u32>() {
                    processes.push(ProcessInfo {
                        pid,
                        name,
                        memory: mem_str.to_string(),
                    });
                }
            }
        }
        
        // Sort by name
        processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        
        Ok(processes)
    }
    #[cfg(not(windows))]
    {
        Err("Not supported on non-Windows yet".to_string())
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct AppConfig {
    id: String,
    name: String,
    #[serde(rename = "executablePath")]
    executable_path: String,
    #[serde(rename = "appType")]
    app_type: String,
    #[serde(rename = "workingDirectory")]
    working_directory: Option<String>,
    #[serde(rename = "arguments")]
    arguments: Option<String>,
    #[serde(rename = "environmentVars")]
    environment_vars: Option<String>,
    #[serde(rename = "icon")]
    icon: Option<String>,
    #[serde(rename = "isRunning")]
    is_running: Option<bool>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct ConfigData {
    apps: Option<Vec<AppConfig>>,
}

fn build_tray_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
    let show = MenuItem::with_id(app, "show", "Show AppCtrl", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    
    let config_json = load_config().unwrap_or_else(|_| "{}".to_string());
    let config: ConfigData = serde_json::from_str(&config_json).unwrap_or_default();
    
    let mut submenu_items = Vec::new();
    
    if let Some(apps) = config.apps {
        for app_conf in apps {
            let is_running = check_process_running(app_conf.executable_path.clone());
            let icon = if is_running { "🟢" } else { "🔴" };
            let title = format!("{} {}", icon, app_conf.name);
            let id = format!("toggle_app:{}", app_conf.id);
            
            if let Ok(item) = MenuItem::with_id(app, &id, &title, true, None::<&str>) {
                submenu_items.push(item);
            }
        }
    }
    
    let mut item_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = Vec::new();
    for item in &submenu_items {
        item_refs.push(item);
    }
    
    let open_submenu = Submenu::with_items(app, "Open", true, &item_refs)?;
    
    Menu::with_items(app, &[&show, &open_submenu, &quit])
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskInfo {
    name: String,
    total_space: u64,
    free_space: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FileInfo {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: u64,
    extension: String,
}

#[cfg(windows)]
unsafe fn hicon_to_base64(hicon: winapi::shared::windef::HICON) -> Result<String, String> {
    use winapi::um::winuser::{GetIconInfo, ICONINFO};
    use winapi::um::wingdi::{
        GetDIBits, CreateCompatibleDC, DeleteDC, GetObjectW, BITMAP, BITMAPINFO, 
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, DeleteObject,
    };
    
    let mut icon_info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut icon_info) == 0 {
        return Err("Failed to get icon info".to_string());
    }
    
    let mut bmp: BITMAP = std::mem::zeroed();
    GetObjectW(
        icon_info.hbmColor as _,
        std::mem::size_of::<BITMAP>() as i32,
        &mut bmp as *mut _ as *mut _,
    );
    
    let width = bmp.bmWidth as usize;
    let height = bmp.bmHeight as usize;
    
    if width == 0 || height == 0 {
        DeleteObject(icon_info.hbmColor as _);
        DeleteObject(icon_info.hbmMask as _);
        return Err("Invalid icon dimensions".to_string());
    }
    
    let hdc = CreateCompatibleDC(std::ptr::null_mut());
    
    let mut bmi: BITMAPINFO = std::mem::zeroed();
    bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width as i32;
    bmi.bmiHeader.biHeight = -(height as i32); // Top-down
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;
    
    let mut pixels: Vec<u8> = vec![0; width * height * 4];
    GetDIBits(
        hdc,
        icon_info.hbmColor,
        0,
        height as u32,
        pixels.as_mut_ptr() as *mut _,
        &mut bmi,
        DIB_RGB_COLORS,
    );
    
    for chunk in pixels.chunks_mut(4) {
        chunk.swap(0, 2);
    }
    
    DeleteDC(hdc);
    DeleteObject(icon_info.hbmColor as _);
    DeleteObject(icon_info.hbmMask as _);
    
    let img = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
        .ok_or("Failed to create image")?;
    
    let mut png_data: Vec<u8> = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;
    
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Ok(format!("data:image/png;base64,{}", b64))
}

#[cfg(windows)]
#[tauri::command]
fn get_disks() -> Result<Vec<DiskInfo>, String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use winapi::um::fileapi::{GetLogicalDriveStringsW, GetDiskFreeSpaceExW};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let mut buffer = [0u16; 256];
        let len = GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr());
        if len == 0 || len > buffer.len() as u32 {
            return Err("Failed to get logical drives".to_string());
        }
        
        let mut disks = Vec::new();
        let mut start = 0;
        for i in 0..len as usize {
            if buffer[i] == 0 {
                if start < i {
                    let drive_utf16 = &buffer[start..i];
                    let os_str = OsString::from_wide(drive_utf16);
                    if let Some(drive_str) = os_str.to_str() {
                        let wide_drive: Vec<u16> = OsStr::new(drive_str)
                            .encode_wide()
                            .chain(std::iter::once(0))
                            .collect();
                            
                        let mut free_bytes_available: winapi::um::winnt::ULARGE_INTEGER = std::mem::zeroed();
                        let mut total_number_of_bytes: winapi::um::winnt::ULARGE_INTEGER = std::mem::zeroed();
                        let mut total_number_of_free_bytes: winapi::um::winnt::ULARGE_INTEGER = std::mem::zeroed();
                        
                        let res = GetDiskFreeSpaceExW(
                            wide_drive.as_ptr(),
                            &mut free_bytes_available,
                            &mut total_number_of_bytes,
                            &mut total_number_of_free_bytes,
                        );
                        
                        let (total_space, free_space) = if res != 0 {
                            (*total_number_of_bytes.QuadPart(), *free_bytes_available.QuadPart())
                        } else {
                            (0, 0)
                        };
                        
                        disks.push(DiskInfo {
                            name: drive_str.to_string(),
                            total_space,
                            free_space,
                        });
                    }
                }
                start = i + 1;
            }
        }
        Ok(disks)
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn get_disks() -> Result<Vec<DiskInfo>, String> {
    Err("Not supported on non-Windows".to_string())
}

#[tauri::command]
fn get_directory_size(path: String) -> Result<u64, String> {
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() || !path_buf.is_dir() {
        return Err("Đường dẫn không hợp lệ hoặc không phải thư mục".to_string());
    }
    
    let mut total = 0;
    let mut stack = vec![path_buf.to_path_buf()];
    
    while let Some(current_path) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(current_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
    }
    Ok(total)
}

#[tauri::command]
fn read_directory(path: String) -> Result<Vec<FileInfo>, String> {
    let path_buf = std::path::Path::new(&path);
    if !path_buf.exists() {
        return Err("Directory does not exist".to_string());
    }
    if !path_buf.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    
    let mut files = Vec::new();
    let entries = std::fs::read_dir(path_buf).map_err(|e| e.to_string())?;
    
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = path.is_dir();
            
            let metadata = entry.metadata().ok();
            let size = if is_dir {
                0
            } else {
                metadata.as_ref().map(|m| m.len()).unwrap_or(0)
            };
            
            let modified = metadata.as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
                
            let extension = path.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
                
            files.push(FileInfo {
                name,
                path: path.to_string_lossy().to_string(),
                is_dir,
                size,
                modified,
                extension,
            });
        }
    }
    
    files.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });
    
    Ok(files)
}

#[cfg(windows)]
#[tauri::command]
fn get_system_icon(path: String, is_dir: bool, use_attr: bool) -> Result<String, String> {
    use winapi::um::shellapi::{SHGetFileInfoW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES, SHFILEINFOW};
    use winapi::um::winuser::DestroyIcon;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide_path: Vec<u16> = OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    unsafe {
        let mut shfi: SHFILEINFOW = std::mem::zeroed();
        let flags = SHGFI_ICON | SHGFI_LARGEICON | if use_attr { SHGFI_USEFILEATTRIBUTES } else { 0 };
        
        let file_attr = if is_dir {
            winapi::um::winnt::FILE_ATTRIBUTE_DIRECTORY
        } else {
            winapi::um::winnt::FILE_ATTRIBUTE_NORMAL
        };
        
        let res = SHGetFileInfoW(
            wide_path.as_ptr(),
            file_attr,
            &mut shfi,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        
        if res == 0 || shfi.hIcon.is_null() {
            return Err("Failed to get icon info from shell".to_string());
        }
        
        let hicon = shfi.hIcon;
        let base64_result = hicon_to_base64(hicon);
        DestroyIcon(hicon);
        base64_result
    }
}

#[cfg(not(windows))]
#[tauri::command]
fn get_system_icon(_path: String, _is_dir: bool, _use_attr: bool) -> Result<String, String> {
    Err("Not supported on non-Windows".to_string())
}

#[cfg(windows)]
#[tauri::command]
fn open_in_explorer(path: String) -> Result<(), String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    
    let path_buf = std::path::PathBuf::from(&path);
    let mut cmd = Command::new("explorer.exe");
    if path_buf.is_file() {
        cmd.raw_arg(format!(r#"/select,"{}""#, path));
    } else {
        cmd.raw_arg(format!(r#""{}""#, path));
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
#[tauri::command]
fn open_in_explorer(_path: String) -> Result<(), String> {
    Err("Not supported on non-Windows".to_string())
}

#[tauri::command]
fn paste_file(src: String, dest_dir: String) -> Result<(), String> {
    let src_path = std::path::Path::new(&src);
    let file_name = src_path.file_name().ok_or("Invalid source file name")?;
    let dest_path = std::path::Path::new(&dest_dir).join(file_name);
    
    if src_path.is_dir() {
        copy_dir_all(src_path, &dest_path).map_err(|e| e.to_string())?;
    } else {
        std::fs::copy(src_path, dest_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_dir_all(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[tauri::command]
fn delete_file(path: String) -> Result<(), String> {
    let path_buf = std::path::Path::new(&path);
    if path_buf.is_dir() {
        std::fs::remove_dir_all(path_buf).map_err(|e| e.to_string())?;
    } else {
        std::fs::remove_file(path_buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LockProcessInfo {
    pid: u32,
    name: String,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
struct RM_UNIQUE_PROCESS {
    dwProcessId: winapi::shared::minwindef::DWORD,
    ProcessStartTime: winapi::shared::minwindef::FILETIME,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
struct RM_PROCESS_INFO {
    Process: RM_UNIQUE_PROCESS,
    strAppName: [winapi::um::winnt::WCHAR; 256],
    strServiceShortName: [winapi::um::winnt::WCHAR; 64],
    ApplicationType: i32,
    AppStatus: winapi::shared::minwindef::ULONG,
    TSSessionId: winapi::shared::minwindef::DWORD,
    bGracefulShutdownRequired: winapi::shared::minwindef::BOOL,
}

#[cfg(windows)]
#[link(name = "rstrtmgr")]
extern "system" {
    fn RmStartSession(
        pSessionHandle: *mut winapi::shared::minwindef::DWORD,
        dwSessionFlags: winapi::shared::minwindef::DWORD,
        strSessionKey: winapi::um::winnt::LPWSTR,
    ) -> winapi::shared::minwindef::DWORD;

    fn RmRegisterResources(
        dwSessionHandle: winapi::shared::minwindef::DWORD,
        nFiles: winapi::shared::minwindef::UINT,
        rgsFileNames: *const winapi::um::winnt::LPCWSTR,
        nApplications: winapi::shared::minwindef::UINT,
        rgApplications: *const RM_UNIQUE_PROCESS,
        nServices: winapi::shared::minwindef::UINT,
        rgsServiceNames: *const winapi::um::winnt::LPCWSTR,
    ) -> winapi::shared::minwindef::DWORD;

    fn RmGetList(
        dwSessionHandle: winapi::shared::minwindef::DWORD,
        pnProcInfoNeeded: *mut winapi::shared::minwindef::UINT,
        pnProcInfo: *mut winapi::shared::minwindef::UINT,
        rgAffectedApps: *mut RM_PROCESS_INFO,
        lpdwRebootReasons: *mut winapi::shared::minwindef::DWORD,
    ) -> winapi::shared::minwindef::DWORD;

    fn RmEndSession(
        dwSessionHandle: winapi::shared::minwindef::DWORD,
    ) -> winapi::shared::minwindef::DWORD;
}

#[cfg(windows)]
fn get_lock_processes(path: &str) -> Result<Vec<LockProcessInfo>, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    
    let wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
        
    unsafe {
        let mut session_handle: winapi::shared::minwindef::DWORD = 0;
        let mut session_key = [0u16; 33];
        
        let res = RmStartSession(&mut session_handle, 0, session_key.as_mut_ptr());
        if res != 0 {
            return Err(format!("RmStartSession failed with error code {}", res));
        }
        
        let file_paths = [wide_path.as_ptr()];
        let res = RmRegisterResources(
            session_handle,
            1,
            file_paths.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        );
        
        if res != 0 {
            RmEndSession(session_handle);
            return Err(format!("RmRegisterResources failed with error code {}", res));
        }
        
        let mut proc_info_needed: winapi::shared::minwindef::UINT = 0;
        let mut proc_info_count: winapi::shared::minwindef::UINT = 0;
        let mut reboot_reasons: winapi::shared::minwindef::DWORD = 0;
        
        let res = RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            std::ptr::null_mut(),
            &mut reboot_reasons,
        );
        
        if res != 0 && res != 234 {
            RmEndSession(session_handle);
            return Ok(Vec::new());
        }
        
        if proc_info_needed == 0 {
            RmEndSession(session_handle);
            return Ok(Vec::new());
        }
        
        proc_info_count = proc_info_needed;
        let mut proc_info_list = vec![std::mem::zeroed::<RM_PROCESS_INFO>(); proc_info_count as usize];
        
        let res = RmGetList(
            session_handle,
            &mut proc_info_needed,
            &mut proc_info_count,
            proc_info_list.as_mut_ptr(),
            &mut reboot_reasons,
        );
        
        if res != 0 {
            RmEndSession(session_handle);
            return Err(format!("RmGetList failed with error code {}", res));
        }
        
        let mut processes = Vec::new();
        for i in 0..proc_info_count as usize {
            let info = &proc_info_list[i];
            let pid = info.Process.dwProcessId;
            
            let len = info.strAppName.iter().position(|&c| c == 0).unwrap_or(info.strAppName.len());
            let app_name = String::from_utf16_lossy(&info.strAppName[..len]);
            
            processes.push(LockProcessInfo {
                pid,
                name: app_name,
            });
        }
        
        RmEndSession(session_handle);
        Ok(processes)
    }
}

#[cfg(windows)]
fn move_to_recycle_bin(path: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::shellapi::{SHFileOperationW, SHFILEOPSTRUCTW, FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FOF_NOERRORUI};
    
    let mut wide_path: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .collect();
    wide_path.push(0);
    wide_path.push(0);
    
    unsafe {
        let mut fileop: SHFILEOPSTRUCTW = std::mem::zeroed();
        fileop.wFunc = FO_DELETE as u32;
        fileop.pFrom = wide_path.as_ptr();
        fileop.fFlags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI;
        
        let res = SHFileOperationW(&mut fileop);
        if res == 0 && fileop.fAnyOperationsAborted == 0 {
            Ok(())
        } else {
            Err(format!("SHFileOperationW failed with code {}", res))
        }
    }
}

#[tauri::command]
fn get_file_lock_processes(path: String) -> Result<Vec<LockProcessInfo>, String> {
    #[cfg(windows)]
    {
        get_lock_processes(&path)
    }
    #[cfg(not(windows))]
    {
        Err("Chỉ hỗ trợ trên Windows".to_string())
    }
}

#[tauri::command]
async fn force_delete_file(path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        
        if let Ok(locks) = get_lock_processes(&path) {
            for lock in locks {
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &lock.pid.to_string()])
                    .creation_flags(0x08000000)
                    .output();
            }
        }
        
        std::thread::sleep(std::time::Duration::from_millis(200));
        
        if let Err(e) = move_to_recycle_bin(&path) {
            let path_buf = std::path::Path::new(&path);
            if path_buf.is_dir() {
                std::fs::remove_dir_all(path_buf).map_err(|err| format!("Xóa thư mục thất bại: {} (Lỗi Recycle Bin: {})", err, e))?;
            } else {
                std::fs::remove_file(path_buf).map_err(|err| format!("Xóa file thất bại: {} (Lỗi Recycle Bin: {})", err, e))?;
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("Chỉ hỗ trợ trên Windows".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Check for local WebView2 Fixed Version
    #[cfg(windows)]
    {
        use std::env;
        if let Ok(current_exe) = env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                let webview2_path = exe_dir.join("WebView2");
                if webview2_path.exists() {
                    env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", webview2_path);
                }
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .manage(ProcessManager::new())
        .manage(AppSettings { minimize_to_tray: Mutex::new(false) })
        .setup(|app| {
            let menu = build_tray_menu(app.handle())?;
            
            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("AppCtrl")
                .on_menu_event(move |app, event| {
                    let id = event.id.as_ref();
                    if id == "show" {
                         show_main_window(app);
                    } else if id == "quit" {
                         app.exit(0);
                    } else if id.starts_with("toggle_app:") {
                         let app_id = id.strip_prefix("toggle_app:").unwrap().to_string();
                         let app_handle = app.clone();
                         
                         tauri::async_runtime::spawn(async move {
                             // Load config to get app details
                             let config_json = load_config().unwrap_or_else(|_| "{}".to_string());
                             let config: ConfigData = serde_json::from_str(&config_json).unwrap_or_default();
                             
                             if let Some(apps) = config.apps {
                                 if let Some(app_conf) = apps.iter().find(|a| a.id == app_id) {
                                     let is_running = check_process_running(app_conf.executable_path.clone());
                                     
                                     if is_running {
                                         // Stop
                                         let _ = stop_app(app_handle.clone(), app_id.clone(), Some(app_conf.executable_path.clone())).await;
                                     } else {
                                         // Start
                                         let _ = start_app(
                                             app_handle.clone(),
                                             app_id.clone(),
                                             app_conf.executable_path.clone(),
                                             app_conf.app_type.clone(),
                                             app_conf.working_directory.clone().unwrap_or_default(),
                                             app_conf.arguments.clone().unwrap_or_default(),
                                             app_conf.environment_vars.clone().unwrap_or_default()
                                         ).await;
                                     }
                                     
                                     // Rebuild and update menu
                                     if let Ok(new_menu) = build_tray_menu(&app_handle) {
                                         if let Some(tray) = app_handle.tray_by_id("main") {
                                             let _ = tray.set_menu(Some(new_menu));
                                         }
                                     }
                                 }
                             }
                         });
                    }
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            
            Ok(())
        })

        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppSettings>();
                let minimize = *state.minimize_to_tray.lock().unwrap();
                
                if minimize {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_app,
            stop_app,
            is_app_running,
            extract_exe_icon,
            check_process_running,
            set_minimize_to_tray,
            load_config,
            save_config,
            get_listening_ports,
            kill_process_by_pid,
            kill_process_by_name,
            get_processes,
            get_disks,
            read_directory,
            get_directory_size,
            get_system_icon,
            open_in_explorer,
            paste_file,
            delete_file,
            get_file_lock_processes,
            force_delete_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
