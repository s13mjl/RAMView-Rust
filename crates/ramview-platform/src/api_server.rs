use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Local, Utc};
use ramview_core::models::ProcessMemoryInfo;
use ramview_core::terminate::is_api_protected;
use serde::Serialize;

use crate::process_monitor::{process_name_for_pid, ProcessMonitor};
use crate::ram_monitor::RAMMonitor;
use crate::terminator::{ProcessTerminator, TerminateOutcome};

const DEFAULT_PORT: u16 = 51888;

/// Lightweight local HTTP server enabling web frontends and browser visualizers
/// to stream live, real hardware RAM metrics and process data.
pub struct LocalApiServer {
    port: u16,
    running: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl LocalApiServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start(
        ram_monitor: Arc<RAMMonitor>,
        process_monitor: Arc<ProcessMonitor>,
        terminator: Arc<ProcessTerminator>,
    ) -> Self {
        Self::start_on_port(ram_monitor, process_monitor, terminator, DEFAULT_PORT)
    }

    pub fn start_on_port(
        ram_monitor: Arc<RAMMonitor>,
        process_monitor: Arc<ProcessMonitor>,
        terminator: Arc<ProcessTerminator>,
        port: u16,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let mut threads = Vec::new();

        let addresses = [format!("127.0.0.1:{port}"), format!("[::1]:{port}")];

        for address in addresses {
            let listener = match TcpListener::bind(&address) {
                Ok(listener) => listener,
                Err(_) => continue,
            };

            if listener.set_nonblocking(true).is_err() {
                continue;
            }

            let running_flag = running.clone();
            let ram = ram_monitor.clone();
            let monitor = process_monitor.clone();
            let term = terminator.clone();

            threads.push(std::thread::spawn(move || {
                while running_flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let ram = ram.clone();
                            let monitor = monitor.clone();
                            let term = term.clone();
                            std::thread::spawn(move || {
                                let _ = stream.set_nonblocking(false);
                                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                                let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                                handle_connection(stream, &ram, &monitor, &term);
                            });
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => {
                            if !running_flag.load(Ordering::Relaxed) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                }
            }));
        }

        Self {
            port,
            running,
            threads,
        }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for LocalApiServer {
    fn drop(&mut self) {
        self.stop();
        for handle in self.threads.drain(..) {
            let _ = handle.join();
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    content_type: Option<&'static str>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, reason: &'static str, body: String) -> Self {
        Self {
            status,
            reason,
            content_type: Some("application/json"),
            body: body.into_bytes(),
        }
    }

    fn empty(status: u16, reason: &'static str) -> Self {
        Self {
            status,
            reason,
            content_type: None,
            body: Vec::new(),
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    ram_monitor: &RAMMonitor,
    process_monitor: &ProcessMonitor,
    terminator: &ProcessTerminator,
) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };

    let response = route(&request, ram_monitor, process_monitor, terminator);
    write_response(&mut stream, &response);
}

fn read_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end;

    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);

        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            head_end = position;
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return None;
        }
    }

    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
    let mut lines = head.split("\r\n");

    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let target = parts.next().unwrap_or("/");

    let path = target
        .split('?')
        .next()
        .unwrap_or("/")
        .to_lowercase()
        .to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body: Vec<u8> = buffer[(head_end + 4).min(buffer.len())..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Some(HttpRequest {
        method,
        path,
        body,
    })
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn route(
    request: &HttpRequest,
    ram_monitor: &RAMMonitor,
    process_monitor: &ProcessMonitor,
    terminator: &ProcessTerminator,
) -> HttpResponse {
    if request.method == "OPTIONS" {
        return HttpResponse::empty(204, "No Content");
    }

    match request.path.as_str() {
        "/api/snapshot" => handle_snapshot(ram_monitor, process_monitor),
        "/api/kill" if request.method == "POST" => {
            handle_kill_process(&request.body, process_monitor, terminator)
        }
        "/api/health" => HttpResponse::json(
            200,
            "OK",
            r#"{"status":"ok","service":"RAMView Local Agent","port":51888}"#.to_string(),
        ),
        _ => HttpResponse::empty(404, "Not Found"),
    }
}

fn handle_snapshot(ram_monitor: &RAMMonitor, process_monitor: &ProcessMonitor) -> HttpResponse {
    let memory = ram_monitor.get_system_memory_snapshot();
    let processes = process_monitor.get_processes(true, true, "All");

    let payload = SnapshotPayload {
        total_physical_bytes: memory.total_physical_bytes,
        used_physical_bytes: memory.used_physical_bytes,
        available_physical_bytes: memory.available_physical_bytes,
        system_memory_usage_percentage: memory.system_memory_usage_percentage(),
        summary_text: memory.summary_text(),
        formatted_total: memory.formatted_total(),
        formatted_used: memory.formatted_used(),
        formatted_available: memory.formatted_available(),
        processes: processes.iter().map(ProcessDto::from).collect(),
    };

    match serde_json::to_string(&payload) {
        Ok(json) => HttpResponse::json(200, "OK", json),
        Err(error) => HttpResponse::json(
            500,
            "Internal Server Error",
            error.to_string(),
        ),
    }
}

fn handle_kill_process(
    body: &[u8],
    process_monitor: &ProcessMonitor,
    terminator: &ProcessTerminator,
) -> HttpResponse {
    let _ = process_monitor;

    let json: serde_json::Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(error) => return json_error(500, "Internal Server Error", &error.to_string()),
    };

    let Some(pid_value) = json.get("pid") else {
        return json_error(400, "Bad Request", "Missing 'pid' in request body");
    };

    let Some(pid) = pid_value.as_u64() else {
        return json_error(500, "Internal Server Error", "The JSON value could not be converted to System.Int32.");
    };
    let pid = pid as u32;

    let Some(process_name) = process_name_for_pid(pid) else {
        return json_success("Process already exited");
    };

    // Safety check: block protected system processes
    if is_api_protected(&process_name) {
        return json_error(
            403,
            "Forbidden",
            &format!("Cannot terminate protected system process '{process_name}'"),
        );
    }

    match terminator.terminate_tree(pid) {
        TerminateOutcome::Terminated => {
            json_success(&format!("Process '{process_name}' (PID {pid}) terminated"))
        }
        TerminateOutcome::AlreadyExited => json_success("Process already exited"),
        TerminateOutcome::AccessDenied => json_error(
            403,
            "Forbidden",
            "Access denied: Access is denied.. Run RAMView as Administrator to terminate elevated processes.",
        ),
        TerminateOutcome::Failed => json_error(
            500,
            "Internal Server Error",
            &format!("Could not terminate process '{process_name}' (PID {pid})."),
        ),
    }
}

fn json_success(message: &str) -> HttpResponse {
    let body = serde_json::json!({ "success": true, "message": message }).to_string();
    HttpResponse::json(200, "OK", body)
}

fn json_error(status: u16, reason: &'static str, message: &str) -> HttpResponse {
    let body = serde_json::json!({ "success": false, "error": message }).to_string();
    HttpResponse::json(status, reason, body)
}

fn write_response(stream: &mut TcpStream, response: &HttpResponse) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n",
        response.status, response.reason
    );

    if let Some(content_type) = response.content_type {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.body.len()
    ));

    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotPayload {
    total_physical_bytes: u64,
    used_physical_bytes: u64,
    available_physical_bytes: u64,
    system_memory_usage_percentage: f64,
    summary_text: String,
    formatted_total: String,
    formatted_used: String,
    formatted_available: String,
    processes: Vec<ProcessDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessDto {
    process_id: u32,
    process_name: String,
    working_set64: i64,
    percentage_of_tracked: f64,
    executable_path: Option<String>,
    icon_png_bytes: Option<String>,
    is_responding: bool,
    start_time: Option<String>,
    timestamp: String,
    instance_count: i32,
    formatted_memory: String,
    formatted_percentage: String,
}

impl From<&ProcessMemoryInfo> for ProcessDto {
    fn from(info: &ProcessMemoryInfo) -> Self {
        Self {
            process_id: info.process_id,
            process_name: info.process_name.clone(),
            working_set64: info.working_set64,
            percentage_of_tracked: info.percentage_of_tracked,
            executable_path: info.executable_path.clone(),
            icon_png_bytes: info.icon_png_bytes.as_ref().map(|bytes| {
                base64::engine::general_purpose::STANDARD.encode(bytes)
            }),
            is_responding: info.is_responding,
            start_time: info.start_time.map(format_local),
            timestamp: format_utc(info.timestamp),
            instance_count: info.instance_count,
            formatted_memory: info.formatted_memory(),
            formatted_percentage: info.formatted_percentage(),
        }
    }
}

fn format_local(value: DateTime<Local>) -> String {
    value.to_rfc3339()
}

fn format_utc(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}
