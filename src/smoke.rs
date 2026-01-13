//! Non-interactive smoke tests for MiniMax media endpoints.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use crate::client::MiniMaxClient;
use crate::modules::{audio, image, music, video};

#[derive(Debug, Clone)]
pub struct SmokeMediaOptions {
    pub output_dir: PathBuf,
    pub image_prompt: String,
    pub image_model: String,
    pub music_prompt: String,
    pub music_model: String,
    pub tts_text: String,
    pub tts_model: String,
    pub video_prompt: String,
    pub video_model: String,
    pub video_duration: u32,
    pub video_resolution: String,
    pub video_async: bool,
    pub skip_image: bool,
    pub skip_music: bool,
    pub skip_tts: bool,
    pub skip_video: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationKind {
    Image,
    Audio,
    Video,
    Json,
}

pub async fn run_smoke_media(
    config: &crate::config::Config,
    options: SmokeMediaOptions,
) -> Result<()> {
    fs::create_dir_all(&options.output_dir).with_context(|| {
        format!(
            "Failed to create output_dir: {}",
            options.output_dir.display()
        )
    })?;

    println!(
        "{} {}",
        "Smoke media output dir:".bold(),
        options.output_dir.display()
    );

    let client = MiniMaxClient::new(config)?;

    let mut image_paths: Vec<PathBuf> = Vec::new();
    let mut music_path: Option<PathBuf> = None;
    let mut tts_path: Option<PathBuf> = None;
    let mut video_path: Option<PathBuf> = None;
    let mut video_task_id: Option<String> = None;

    if !options.skip_image {
        println!("{}", "Generating image...".bold());
        let paths = image::generate(
            &client,
            image::ImageGenerateOptions {
                model: options.image_model.clone(),
                prompt: options.image_prompt.clone(),
                negative_prompt: None,
                aspect_ratio: None,
                width: None,
                height: None,
                style: None,
                response_format: None,
                seed: None,
                n: None,
                prompt_optimizer: None,
                subject_reference: Vec::new(),
                output_dir: options.output_dir.clone(),
            },
        )
        .await?;

        for path in &paths {
            validate_generated_file(path, ValidationKind::Image)?;
        }
        image_paths = paths;
    }

    if !options.skip_music {
        println!("{}", "Generating music...".bold());
        let path = music::generate(
            &client,
            music::MusicGenerateOptions {
                model: options.music_model.clone(),
                prompt: options.music_prompt.clone(),
                lyrics: None,
                stream: false,
                output_format: None,
                audio_setting_json: None,
                output_dir: options.output_dir.clone(),
            },
        )
        .await?;

        validate_generated_file(&path, ValidationKind::Audio)?;
        music_path = Some(path);
    }

    if !options.skip_tts {
        println!("{}", "Generating TTS...".bold());
        let path = audio::t2a(
            &client,
            audio::T2aOptions {
                model: options.tts_model.clone(),
                text: options.tts_text.clone(),
                stream: false,
                output_format: None,
                voice_id: None,
                voice_setting_json: None,
                audio_setting_json: None,
                pronunciation_dict_json: None,
                timber_weights_json: None,
                language_boost_json: None,
                voice_modify_json: None,
                subtitle_enable: None,
                output_dir: options.output_dir.clone(),
            },
        )
        .await?;

        validate_generated_file(&path, ValidationKind::Audio)?;
        tts_path = Some(path);
    }

    if !options.skip_video {
        println!("{}", "Generating video...".bold());
        let (video_duration, video_resolution) =
            normalize_video_options(options.video_duration, &options.video_resolution)?;
        let result = video::generate(
            &client,
            video::VideoGenerateOptions {
                model: options.video_model.clone(),
                prompt: options.video_prompt.clone(),
                first_frame: None,
                last_frame: None,
                subject_reference: Vec::new(),
                subject_reference_json: None,
                duration: Some(video_duration),
                resolution: Some(video_resolution),
                callback_url: None,
                prompt_optimizer: None,
                fast_pretreatment: None,
                wait: !options.video_async,
                output_dir: options.output_dir.clone(),
            },
        )
        .await?;

        video_task_id = result.task_id;
        if let Some(path) = result.video_path {
            validate_generated_file(&path, ValidationKind::Video)?;
            video_path = Some(path);
        } else if let Some(path) = result.response_path {
            validate_generated_file(&path, ValidationKind::Json)?;
        }
    }

    println!();
    println!("{}", "Smoke test results".bold().cyan());
    println!("{}", "==================".cyan());
    if !options.skip_image {
        if image_paths.is_empty() {
            println!("  {} image: (none)", "!".yellow());
        } else {
            for path in &image_paths {
                println!("  {} image: {}", "✓".green(), path.display());
            }
        }
    }
    if !options.skip_music {
        println!(
            "  {} music: {}",
            music_path
                .as_ref()
                .map_or_else(|| "!".yellow(), |_| "✓".green()),
            music_path
                .as_ref()
                .map_or_else(|| "(none)".to_string(), |p| p.display().to_string())
        );
    }
    if !options.skip_tts {
        println!(
            "  {} tts: {}",
            tts_path
                .as_ref()
                .map_or_else(|| "!".yellow(), |_| "✓".green()),
            tts_path
                .as_ref()
                .map_or_else(|| "(none)".to_string(), |p| p.display().to_string())
        );
    }
    if !options.skip_video {
        if let Some(path) = &video_path {
            println!("  {} video: {}", "✓".green(), path.display());
        } else if let Some(task_id) = &video_task_id {
            println!(
                "  {} video: submitted task_id={task_id} (async)",
                "·".dimmed()
            );
        } else {
            println!("  {} video: (none)", "!".yellow());
        }
    }

    Ok(())
}

fn validate_generated_file(path: &Path, expected: ValidationKind) -> Result<()> {
    validate_file(path, expected)
}

fn validate_file(path: &Path, expected: ValidationKind) -> Result<()> {
    let data =
        fs::read(path).with_context(|| format!("Failed to read output file {}", path.display()))?;
    if data.is_empty() {
        anyhow::bail!("Generated file is empty: {}", path.display());
    }

    if looks_like_json(&data) && expected != ValidationKind::Json {
        let snippet = String::from_utf8_lossy(&data[..data.len().min(512)]).to_string();
        anyhow::bail!(
            "Generated file looks like JSON (unexpected for {expected:?}): {}. First bytes: {snippet}",
            path.display()
        );
    }

    let detected = detect_kind(&data);
    match expected {
        ValidationKind::Json => {
            if !looks_like_json(&data) {
                println!(
                    "{} {} (expected JSON, detected {:?})",
                    "!".yellow(),
                    path.display(),
                    detected
                );
            }
        }
        ValidationKind::Image => {
            if matches!(detected, DetectedKind::Image) {
                // ok
            } else {
                println!(
                    "{} {} (expected image, detected {:?})",
                    "!".yellow(),
                    path.display(),
                    detected
                );
            }
        }
        ValidationKind::Audio => {
            if matches!(detected, DetectedKind::Audio) {
                // ok
            } else {
                println!(
                    "{} {} (expected audio, detected {:?})",
                    "!".yellow(),
                    path.display(),
                    detected
                );
            }
        }
        ValidationKind::Video => {
            if matches!(detected, DetectedKind::Video) {
                // ok
            } else {
                println!(
                    "{} {} (expected video, detected {:?})",
                    "!".yellow(),
                    path.display(),
                    detected
                );
            }
        }
    }

    Ok(())
}

fn looks_like_json(data: &[u8]) -> bool {
    let first = data.iter().copied().find(|b| !b.is_ascii_whitespace());
    matches!(first, Some(b'{') | Some(b'['))
}

#[derive(Debug)]
enum DetectedKind {
    Image,
    Audio,
    Video,
    Json,
    Unknown,
}

fn detect_kind(data: &[u8]) -> DetectedKind {
    if looks_like_json(data) {
        return DetectedKind::Json;
    }

    // Images
    if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        return DetectedKind::Image;
    }
    if data.len() >= 3 && &data[..3] == b"\xFF\xD8\xFF" {
        return DetectedKind::Image;
    }
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return DetectedKind::Image;
    }

    // Audio
    if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return DetectedKind::Audio;
    }
    if data.starts_with(b"ID3")
        || data.starts_with(&[0xFF, 0xFB])
        || data.starts_with(&[0xFF, 0xF3])
        || data.starts_with(&[0xFF, 0xF2])
    {
        return DetectedKind::Audio;
    }
    if data.starts_with(b"fLaC") {
        return DetectedKind::Audio;
    }
    if data.starts_with(b"OggS") {
        return DetectedKind::Audio;
    }

    // Video (MP4)
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        return DetectedKind::Video;
    }

    DetectedKind::Unknown
}

fn normalize_video_options(duration: u32, resolution: &str) -> Result<(u32, String)> {
    let duration = match duration {
        6 | 10 => duration,
        5 => {
            println!("{} duration=5s is not supported; using 6s", "·".dimmed());
            6
        }
        other => anyhow::bail!("Invalid video duration {other}. Supported: 6 or 10 seconds."),
    };

    let trimmed = resolution.trim();
    let upper = trimmed.to_ascii_uppercase();
    let normalized = match upper.as_str() {
        "" => "768P".to_string(),
        "512P" | "512" => "512P".to_string(),
        "768P" | "768" => "768P".to_string(),
        "1080P" | "1080" => "1080P".to_string(),
        "720P" | "720" | "720P@60" | "720P60" => {
            println!(
                "{} resolution=720p is not supported; using 768P",
                "·".dimmed()
            );
            "768P".to_string()
        }
        other => anyhow::bail!(
            "Invalid video resolution {other}. Supported: 512P, 768P, 1080P (720p maps to 768P)."
        ),
    };

    Ok((duration, normalized))
}
