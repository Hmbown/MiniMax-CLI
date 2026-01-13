---
name: voice-podcast-kit
description: Create multi-host podcast episodes with cloned voices, intro music, and transitions.
allowed-tools: voice_clone, voice_list, tts, generate_music, list_dir, upload_file
---
You are running the Voice Podcast Kit skill.

Goal
- Produce a complete podcast episode with distinct cloned voices for each host/guest, intro/outro music, and smooth transitions between segments.

Ask for
- Episode topic and title.
- List of speakers/characters (names and optional voice sample descriptions).
- Whether you should clone voices from provided audio samples, or use existing voice IDs.
- Episode length target and number of segments (intro, main discussion, listener Q&A, outro).
- Any music preferences (genre, mood, tempo).

Workflow
1) Confirm speaker lineup and collect voice samples if cloning is requested:
   - If audio files are provided, call voice_clone for each speaker.
   - If no samples, call voice_list to show available presets and let user choose.
2) Draft a segment-by-segment outline with speaker assignments and timing.
3) Generate intro/outro music:
   - Call generate_music with appropriate mood (upbeat for intro, winding down for outro).
4) Write scripts for each segment with clear speaker labels.
5) For each spoken segment:
   - Call tts with the correct voice_id for each speaker.
   - Use output_format "mp3" for smooth editing.
6) Optionally add transition sounds or music beds between segments.
7) Return a production清单:
   - Music files (intro/outro)
   - Each spoken segment as separate audio files
   - Full episode script text
   - Assembly suggestions (timeline order)

Response style
- Keep track of which voice_id maps to which speaker name clearly.
- Provide file paths organized by segment and type.
- Suggest editing workflow but leave final assembly to user.

Notes
- Voice consistency across episodes is a key value proposition—encourage users to save voice IDs for recurring hosts.
- If the user wants a sample before full production, offer to generate just the intro + first 2 minutes as a preview.
