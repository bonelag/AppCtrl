use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    menu::{Menu, MenuItem},
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
            let show = MenuItem::with_id(app, "show", "Show AppCtrl", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("AppCtrl")
                .on_menu_event(move |app, event| {
                    match event.id.as_ref() {
                        "show" => show_main_window(app),
                        "quit" => app.exit(0),
                        _ => {}
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
            get_processes
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
