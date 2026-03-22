use std::fs;
use std::io::{copy, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use rusty_ytdl::blocking::Video as BlockingYoutubeVideo;
use rusty_ytdl::{VideoOptions, VideoQuality, VideoSearchOptions};
use serde::Serialize;
use tauri::Emitter;

#[derive(Serialize, Clone)]
struct DownloadProgressPayload {
    job_id: u32,
    progress: Option<f32>,
    message: String,
}

fn is_supported_social_host(host: &str) -> bool {
    host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "youtu.be"
        || host == "instagram.com"
        || host == "www.instagram.com"
        || host == "facebook.com"
        || host == "www.facebook.com"
        || host == "m.facebook.com"
        || host == "fb.watch"
}

fn is_youtube_host(host: &str) -> bool {
    host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "youtu.be"
}

fn canonicalize_youtube_url(input: &str) -> String {
    let parsed = match reqwest::Url::parse(input) {
        Ok(url) => url,
        Err(_) => return input.to_string(),
    };

    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "youtu.be" {
        if let Some(id) = parsed.path_segments().and_then(|mut s| s.next()) {
            if !id.trim().is_empty() {
                return format!("https://www.youtube.com/watch?v={}", id.trim());
            }
        }
    }

    if host == "youtube.com" || host == "www.youtube.com" || host == "m.youtube.com" {
        if let Some(v) = parsed.query_pairs().find(|(k, _)| k == "v") {
            return format!("https://www.youtube.com/watch?v={}", v.1);
        }

        let parts: Vec<String> = parsed
            .path_segments()
            .map(|segments| segments.map(|s| s.to_string()).collect())
            .unwrap_or_default();
        if parts.len() >= 2 && parts[0] == "shorts" {
            return format!("https://www.youtube.com/watch?v={}", parts[1]);
        }
    }

    input.to_string()
}

fn resolve_destination_dir(destination_dir: Option<String>) -> PathBuf {
    if let Some(raw_dir) = destination_dir {
        let trimmed = raw_dir.trim().trim_matches('"');
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    dirs::download_dir().unwrap_or_else(std::env::temp_dir)
}

fn ytdlp_local_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn command_available(binary: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .output()
        .map(|result| result.status.success())
        .unwrap_or(false)
}

fn current_exe_sidecar_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            candidates.push(parent.join(ytdlp_local_binary_name()));
            if let Ok(entries) = fs::read_dir(parent) {
                let exe_ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with("yt-dlp-") && name.ends_with(exe_ext) {
                        candidates.push(entry.path());
                    }
                }
            }
        }
    }
    candidates
}

fn dev_sidecar_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let manifest = PathBuf::from(manifest_dir);
        candidates.push(manifest.join("binaries").join(ytdlp_local_binary_name()));
        candidates.push(
            manifest
                .parent()
                .unwrap_or(&manifest)
                .join("binaries")
                .join(ytdlp_local_binary_name()),
        );
    }
    candidates
}

fn ensure_ytdlp_command() -> Result<String, String> {
    if command_available("yt-dlp") {
        return Ok("yt-dlp".to_string());
    }

    for path in current_exe_sidecar_candidates()
        .into_iter()
        .chain(dev_sidecar_candidates().into_iter())
    {
        if path.exists() {
            let candidate = path.to_string_lossy().to_string();
            if command_available(&candidate) {
                return Ok(candidate);
            }
        }
    }

    Err("yt-dlp binary not found. Place it in PATH or src-tauri/binaries for sidecar packaging.".to_string())
}

fn emit_download_progress(
    app_handle: Option<&tauri::AppHandle>,
    job_id: Option<u32>,
    progress: Option<f32>,
    message: String,
) {
    if let (Some(app), Some(id)) = (app_handle, job_id) {
        let _ = app.emit(
            "bolt-download-progress",
            DownloadProgressPayload {
                job_id: id,
                progress,
                message,
            },
        );
    }
}

fn parse_progress_percent(line: &str) -> Option<f32> {
    let percent_idx = line.find('%')?;
    let mut start = percent_idx;

    while start > 0 {
        let c = line.as_bytes()[start - 1] as char;
        if c.is_ascii_digit() || c == '.' {
            start -= 1;
        } else {
            break;
        }
    }

    if start == percent_idx {
        return None;
    }

    let value = line[start..percent_idx].trim().parse::<f32>().ok()?;
    Some(value.clamp(0.0, 100.0))
}

fn ensure_mp4_extension(filename: String) -> String {
    if Path::new(&filename).extension().is_some() {
        filename
    } else {
        format!("{}.mp4", filename)
    }
}

fn native_youtube_download(
    trimmed_url: &str,
    base_dir: &PathBuf,
    filename: Option<String>,
) -> Result<String, String> {
    let video_options = VideoOptions {
        quality: VideoQuality::Highest,
        filter: VideoSearchOptions::VideoAudio,
        ..Default::default()
    };

    let canonical_url = canonicalize_youtube_url(trimmed_url);
    let video = BlockingYoutubeVideo::new_with_options(&canonical_url, video_options)
        .map_err(|e| format!("Failed to initialize YouTube download: {}", e))?;

    let resolved_filename = filename
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(sanitize_filename)
        .map(ensure_mp4_extension)
        .unwrap_or_else(|| {
            let inferred = video
                .get_info()
                .ok()
                .map(|info| sanitize_filename(&info.video_details.title))
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| infer_filename(&canonical_url));
            ensure_mp4_extension(inferred)
        });

    let target_path = unique_target_path(base_dir, &resolved_filename);

    video
        .download(&target_path)
        .map_err(|e| format!("YouTube download failed: {}", e))?;

    Ok(target_path.to_string_lossy().to_string())
}

fn ytdlp_social_download(
    trimmed_url: &str,
    base_dir: &PathBuf,
    filename: Option<String>,
    playlist_mode: bool,
    playlist_folder: Option<String>,
    app_handle: Option<&tauri::AppHandle>,
    job_id: Option<u32>,
) -> Result<String, String> {
    let ytdlp_cmd = ensure_ytdlp_command()?;

    emit_download_progress(
        app_handle,
        job_id,
        Some(0.0),
        "Preparing downloader".to_string(),
    );

    let playlist_dir = if playlist_mode {
        let folder = playlist_folder
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(sanitize_filename)
            .unwrap_or_else(|| "%(playlist_title|Playlist).180B".to_string());
        Some(base_dir.join(folder))
    } else {
        None
    };

    let output_template = if let Some(dir) = playlist_dir.as_ref() {
        dir.join("%(playlist_index|0)03d - %(title).180B [%(id)s].%(ext)s")
    } else if let Some(raw_name) = filename.as_deref() {
        let trimmed = raw_name.trim();
        if trimmed.is_empty() {
            base_dir.join("%(title).180B [%(id)s].%(ext)s")
        } else {
            let clean = sanitize_filename(trimmed);
            base_dir.join(format!("{}.%(ext)s", clean))
        }
    } else {
        base_dir.join("%(title).180B [%(id)s].%(ext)s")
    };

    let mut child = Command::new(&ytdlp_cmd)
        .args(if playlist_mode {
            vec!["--yes-playlist"]
        } else {
            vec!["--no-playlist"]
        })
        .arg("--newline")
        .arg("--restrict-filenames")
        .arg("--print")
        .arg("after_move:filepath")
        .arg("-o")
        .arg(&output_template)
        .arg(trimmed_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run yt-dlp executable ({}): {}", ytdlp_cmd, e))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture yt-dlp stderr".to_string())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture yt-dlp stdout".to_string())?;

    for line in BufReader::new(stderr).lines() {
        let line = match line {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(percent) = parse_progress_percent(&line) {
            emit_download_progress(app_handle, job_id, Some(percent), line.clone());
        } else if line.contains("[download]") {
            emit_download_progress(app_handle, job_id, None, line.clone());
        }
    }

    let status = child
        .wait()
        .map_err(|e| format!("Failed waiting for yt-dlp process: {}", e))?;

    let mut stdout_content = String::new();
    stdout
        .read_to_string(&mut stdout_content)
        .map_err(|e| format!("Failed reading yt-dlp stdout: {}", e))?;

    if !status.success() {
        let detail = stdout_content.trim();
        if detail.is_empty() {
            return Err("Social download failed with unknown error".to_string());
        }
        return Err(format!("Social download failed: {}", detail));
    }

    let final_path = stdout_content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .map(ToOwned::to_owned);

    emit_download_progress(
        app_handle,
        job_id,
        Some(100.0),
        "Download completed".to_string(),
    );

    if playlist_mode {
        if let Some(dir) = playlist_dir {
            return Ok(dir.to_string_lossy().to_string());
        }
    }

    if let Some(path) = final_path {
        Ok(path)
    } else {
        Ok(base_dir.to_string_lossy().to_string())
    }
}

fn sanitize_filename(raw: &str) -> String {
    let filtered: String = raw
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .filter(|c| !c.is_control())
        .collect();

    let trimmed = filtered.trim_matches([' ', '.']);
    if trimmed.is_empty() {
        "download.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn infer_filename(url: &str) -> String {
    let parsed = match reqwest::Url::parse(url) {
        Ok(value) => value,
        Err(_) => return "download.bin".to_string(),
    };

    let name_from_path = parsed
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
        .unwrap_or("download.bin");

    sanitize_filename(name_from_path)
}

fn unique_target_path(base_dir: &PathBuf, filename: &str) -> PathBuf {
    let candidate = base_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let source = Path::new(filename);
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = source.extension().and_then(|s| s.to_str()).unwrap_or("");

    for idx in 1..=10_000 {
        let numbered = if ext.is_empty() {
            format!("{} ({})", stem, idx)
        } else {
            format!("{} ({}).{}", stem, idx, ext)
        };

        let candidate = base_dir.join(numbered);
        if !candidate.exists() {
            return candidate;
        }
    }

    base_dir.join(format!("{}_final", filename))
}

#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let cleaned = trimmed.trim_matches('"').replace('/', "\\");
    let input_path = PathBuf::from(cleaned);
    let target = fs::canonicalize(&input_path).unwrap_or(input_path);

    if target.is_file() {
        Command::new("explorer")
            .arg("/select,")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to reveal file in Explorer: {}", e))
    } else if target.is_dir() {
        Command::new("explorer")
            .arg(&target)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to open folder in Explorer: {}", e))
    } else {
        Err(format!("Path does not exist: {}", target.display()))
    }
}

#[tauri::command]
pub fn save_filter_file(path: String, content: String) -> Result<(), String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create filter directory: {}", e))?;
        }
    }

    fs::write(&target, content).map_err(|e| format!("Failed to save filter file: {}", e))
}

#[tauri::command]
pub fn load_filter_file(path: String) -> Result<String, String> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Path is empty".to_string());
    }

    let target = PathBuf::from(trimmed);
    fs::read_to_string(&target).map_err(|e| format!("Failed to load filter file: {}", e))
}

#[tauri::command]
pub fn download_file(
    url: String,
    destination_dir: Option<String>,
    filename: Option<String>,
) -> Result<String, String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        return Err("URL is empty".to_string());
    }

    let parsed = reqwest::Url::parse(trimmed_url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err("Only HTTP(S) URLs are supported".to_string());
        }
    }

    let base_dir = resolve_destination_dir(destination_dir);

    fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to prepare destination directory: {}", e))?;

    let resolved_filename = filename
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(sanitize_filename)
        .unwrap_or_else(|| infer_filename(trimmed_url));
    let target_path = unique_target_path(&base_dir, &resolved_filename);

    let mut response = reqwest::blocking::Client::new()
        .get(trimmed_url)
        .send()
        .map_err(|e| format!("Failed to start download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status {}", response.status()));
    }

    let mut output = fs::File::create(&target_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    copy(&mut response, &mut output)
        .map_err(|e| format!("Failed to write downloaded file: {}", e))?;

    Ok(target_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn social_download(
    app_handle: tauri::AppHandle,
    url: String,
    destination_dir: Option<String>,
    filename: Option<String>,
    playlist_mode: Option<bool>,
    playlist_folder: Option<String>,
    job_id: Option<u32>,
) -> Result<String, String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        return Err("URL is empty".to_string());
    }

    let parsed = reqwest::Url::parse(trimmed_url).map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed
        .host_str()
        .map(|v| v.to_lowercase())
        .ok_or_else(|| "URL host is missing".to_string())?;

    if !is_supported_social_host(&host) {
        return Err("Supported sources are YouTube, Instagram, and Facebook".to_string());
    }

    let base_dir = resolve_destination_dir(destination_dir);
    let is_playlist_mode = playlist_mode.unwrap_or(false);

    fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to prepare destination directory: {}", e))?;

    if is_youtube_host(&host) {
        let canonical = canonicalize_youtube_url(trimmed_url);

        return match ytdlp_social_download(
            &canonical,
            &base_dir,
            filename.clone(),
            is_playlist_mode,
            playlist_folder.clone(),
            Some(&app_handle),
            job_id,
        ) {
            Ok(path) => Ok(path),
            Err(ytdlp_error) => {
                if is_playlist_mode {
                    return Err(format!(
                        "YouTube playlist download failed ({ytdlp_error})"
                    ));
                }
                match native_youtube_download(&canonical, &base_dir, filename) {
                    Ok(path) => Ok(path),
                    Err(native_error) => Err(format!(
                        "YouTube yt-dlp downloader failed ({ytdlp_error}). Fallback native failed ({native_error})"
                    )),
                }
            }
        };
    }

    ytdlp_social_download(
        trimmed_url,
        &base_dir,
        filename,
        is_playlist_mode,
        playlist_folder,
        Some(&app_handle),
        job_id,
    )
}
