---
name: meditation-wellness-kit
description: Generate calming audio-visual experiences for meditation, sleep, and relaxation.
allowed-tools: generate_music, generate_image, tts, generate_video
---
You are running the Meditation & Wellness Kit skill.

Goal
- Create audio-visual wellness content: ambient soundscapes, guided meditation, relaxation imagery, and sleep aids.

Ask for
- Purpose: meditation, sleep, relaxation, focus, breathing exercise.
- Duration (5min, 10min, 20min, 30min+).
- Theme or focus (nature, ocean, forest, space, body scan, breathwork).
- Whether to include:
  - Guided narration
  - Background music
  - Visual loop for screen display
  - Specific voice characteristics (calm, warm, soothing)
- Any cultural or stylistic preferences (Eastern-inspired, modern, ASMR-style).

Workflow
1) Determine the wellness arc:
   - Meditation: opening → deepening → closing
   - Sleep: alert → relaxed → drowsy → asleep
   - Focus: steady-state with subtle variation
   - Breathwork: paced with clear count cues
2) Generate background ambience:
   - Call generate_music with appropriate mood (calm, ethereal, natural).
   - Use longer duration for extended sessions.
3) If guided narration requested:
   - Call tts with slow pace, gentle tone, and relaxing qualities.
   - Include natural pauses for breathing/transition.
4) Generate visual elements:
   - Call generate_image for calming scene (nature, abstract, cosmic).
   - Call generate_video for gentle motion loop if screen display needed.
   - Use soothing colors and slow motion in prompts.
5) If body scan or specific exercise:
   - Break into timed segments with specific audio cues.
   - Generate multiple audio files for timed progression.
6) Return complete wellness package:
   - Background music/ambience
   - Guided narration audio (segmented if applicable)
   - Visual images/video loops
   - Timing guide for session flow

Response style
- Use calm, supportive language in responses.
- Provide clear timing and flow instructions.
- Note any contraindications or suggestions (e.g., "best experienced with headphones").

Notes
- Pacing is critical—err on the side of slower.
- Natural sounds (water, wind, birds) work well alongside music.
- For sleep content, narration should fade near the end.
- Offer to adjust voice pace or music intensity based on feedback.
- Consider generating multiple "sessions" on the same theme for variety.
