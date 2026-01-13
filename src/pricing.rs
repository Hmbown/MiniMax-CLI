//! Cost estimation for `MiniMax` API tools

use serde_json::Value;

/// `MiniMax` API Pricing constants (as of 2024)
pub mod prices {
    /// Image generation: $0.0035 per image
    pub const IMAGE_PER_UNIT: f64 = 0.0035;

    /// Audio TTS (turbo): ~$0.00006 per character ($60/M)
    pub const AUDIO_TURBO_PER_CHAR: f64 = 0.00006;
    /// Audio TTS (HD): ~$0.0001 per character ($100/M)
    pub const AUDIO_HD_PER_CHAR: f64 = 0.0001;

    /// Video generation base prices
    pub const VIDEO_768P_6S: f64 = 0.19;
    pub const VIDEO_768P_10S: f64 = 0.25;
    pub const VIDEO_1080P_6S: f64 = 0.27;
    pub const VIDEO_1080P_10S: f64 = 0.33;

    /// Music generation: $0.03 per 5-minute composition
    pub const MUSIC_PER_5MIN: f64 = 0.03;

    /// Voice cloning: $3.00 per voice
    pub const VOICE_CLONE: f64 = 3.00;
}

/// Estimated cost for a tool execution
#[derive(Debug, Clone)]
pub struct CostEstimate {
    /// Minimum cost in USD
    pub min_usd: f64,
    /// Maximum cost in USD
    pub max_usd: f64,
    /// Cost breakdown explanation
    pub breakdown: String,
}

impl CostEstimate {
    #[must_use]
    pub fn new(min_usd: f64, max_usd: f64, breakdown: impl Into<String>) -> Self {
        Self {
            min_usd,
            max_usd,
            breakdown: breakdown.into(),
        }
    }

    #[must_use]
    pub fn fixed(usd: f64, breakdown: impl Into<String>) -> Self {
        Self::new(usd, usd, breakdown)
    }

    /// Format the cost for display
    #[must_use]
    pub fn display(&self) -> String {
        if (self.min_usd - self.max_usd).abs() < 0.0001 {
            format!("${:.4}", self.min_usd)
        } else {
            format!("${:.4} - ${:.4}", self.min_usd, self.max_usd)
        }
    }
}

/// Estimate cost for image generation
#[must_use]
pub fn estimate_image_cost(_params: &Value) -> CostEstimate {
    CostEstimate::fixed(
        prices::IMAGE_PER_UNIT,
        "Image generation: $0.0035 per image",
    )
}

/// Estimate cost for audio/TTS generation
#[must_use]
pub fn estimate_audio_cost(params: &Value) -> CostEstimate {
    let text_len = params
        .get("text")
        .and_then(|t| t.as_str())
        .map_or(100usize, str::len);
    let text_len_f64 = f64::from(u32::try_from(text_len).unwrap_or(u32::MAX));

    let min = text_len_f64 * prices::AUDIO_TURBO_PER_CHAR;
    let max = text_len_f64 * prices::AUDIO_HD_PER_CHAR;

    CostEstimate::new(
        min,
        max,
        format!("TTS: ~{text_len} chars @ $0.00006-0.0001/char"),
    )
}

/// Estimate cost for video generation
#[must_use]
pub fn estimate_video_cost(params: &Value) -> CostEstimate {
    let duration = params
        .get("duration")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(6);
    let resolution = params
        .get("resolution")
        .and_then(|r| r.as_str())
        .unwrap_or("768P");

    let cost = match (resolution, duration) {
        ("768P", d) if d <= 6 => prices::VIDEO_768P_6S,
        ("768P", _) => prices::VIDEO_768P_10S,
        ("1080P", d) if d <= 6 => prices::VIDEO_1080P_6S,
        ("1080P", _) => prices::VIDEO_1080P_10S,
        _ => prices::VIDEO_768P_6S,
    };

    CostEstimate::fixed(cost, format!("Video {resolution}@{duration}s: ${cost:.2}"))
}

/// Estimate cost for music generation
#[must_use]
pub fn estimate_music_cost(params: &Value) -> CostEstimate {
    let duration_secs = params
        .get("duration")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(30)
        .max(1);

    // Price is per 5-minute (300s) chunk
    let duration_secs = u64::try_from(duration_secs).unwrap_or(1);
    let chunks = duration_secs.div_ceil(300);
    let chunks_f64 = f64::from(u32::try_from(chunks).unwrap_or(u32::MAX));
    let cost = chunks_f64 * prices::MUSIC_PER_5MIN;

    CostEstimate::fixed(
        cost,
        format!("Music: {duration_secs}s @ $0.03 per 5-min block"),
    )
}

/// Estimate cost for voice cloning
#[must_use]
pub fn estimate_voice_clone_cost(_params: &Value) -> CostEstimate {
    CostEstimate::fixed(prices::VOICE_CLONE, "Voice cloning: $3.00 per voice")
}

/// Get cost estimate for a tool by name
#[must_use]
pub fn estimate_tool_cost(tool_name: &str, params: &Value) -> Option<CostEstimate> {
    match tool_name {
        "generate_image" => Some(estimate_image_cost(params)),
        "tts" | "tts_async_create" => Some(estimate_audio_cost(params)),
        "generate_video" => Some(estimate_video_cost(params)),
        "generate_music" => Some(estimate_music_cost(params)),
        "voice_clone" => Some(estimate_voice_clone_cost(params)),
        _ => None,
    }
}
