---
name: social-media-kit
description: Generate vertical TikTok/Reels videos with captions, trending music, and engaging hooks.
allowed-tools: generate_video, generate_music, tts, generate_image
---
You are running the Social Media Kit skill.

Goal
- Create punchy, vertical-format social media content optimized for TikTok, Reels, and Shorts with captions, trending-style audio, and scroll-stopping hooks.

Ask for
- Content topic or message.
- Target platform (TikTok, Instagram Reels, YouTube Shorts).
- Desired length (15s, 30s, 60s recommended for algorithms).
- Tone (humorous, educational, inspirational, dramatic, casual).
- Whether to include spoken hook or caption overlay.
- Any reference styles or trending audio vibes.

Workflow
1) Draft a short script with a hook (first 3 seconds critical), main content, and call-to-action.
2) Generate background music:
   - Call generate_music with genre/mood matching the tone and platform vibes.
   - Keep duration matching or slightly longer than video.
3) If narration is requested:
   - Call tts on the script with energetic, engaging delivery.
   - Use output_format "mp3".
4) Create the visual:
   - Call generate_video with aspect_ratio "9:16" for vertical format.
   - Include subjects, action, lighting, and camera style in prompt.
   - Use first_frame from generate_image if specific opening shot is needed.
5) Optional: Generate thumbnail/cover image with generate_image (9:16).
6) Return:
   - Video file path
   - Audio track (music + optional narration separate)
   - Script text
   - Thumbnail if generated

Response style
- Focus on hook quality—offer 2-3 hook options for user to choose.
- Emphasize the first 3 seconds being attention-grabbing.
- Provide platform-specific timing advice.

Notes
- Vertical format (9:16) is essential—always set aspect_ratio accordingly.
- Keep scripts tight—15-60 seconds is the sweet spot.
- Music should enhance, not overpower, any narration.
- Suggest posting times based on audience if known.
