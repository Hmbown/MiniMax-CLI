---
name: voiceover-studio
description: Design custom voices from text prompts and produce professional narration with voice design previews.
allowed-tools: voice_design, voice_list, tts, tts_async_create, tts_async_query, retrieve_file, download_file
---
You are running the Voiceover Studio skill.

Goal
- Design a custom voice from text descriptions, preview it, and produce full professional narration for any project.

Ask for
- Project type: commercial, documentary, animation, corporate, audiobook, podcast.
- Voice characteristics (age, gender, accent, tone, personality).
- Usage context (broadcast, online, telephone, character voice).
- Script content or source (file upload or text input).
- Duration estimate (short spot vs. long-form content).
- Whether to:
  - Design a new voice from scratch
  - Browse existing voices and customize
  - Clone from provided samples
- Quality preference (speech-02-hd for premium, speech-02-turbo for speed).

Workflow
1) Determine voice approach:
   - If designing new: call voice_design with descriptive prompt (e.g., "warm中年 male voice, slight Southern accent, trustworthy and friendly").
   - If browsing: call voice_list to show options with characteristics.
   - If cloning: request audio sample and call voice_clone.
2) Generate preview samples:
   - Call tts with sample text (2-3 sentences covering different emotions).
   - Offer 2-3 voice variations for comparison.
   - Get user feedback and iterate on voice design if needed.
3) Finalize voice selection:
   - Confirm voice_id to use for full production.
   - Note any specific direction for delivery (energetic, whisper, authoritative).
4) Process full script:
   - If short (<5min): call tts directly with full script.
   - If long: call tts_async_create with script or uploaded file.
   - Poll with tts_async_query until complete.
   - Download with retrieve_file or download_file.
5) Optional: Generate alternate versions:
   - Different takes or emotional deliveries.
   - "Radio edit" (shorter, punchier version) for advertising.
6) Return production package:
   - Voice design specifications (for future consistency)
   - Preview audio files
   - Final narration audio
   - Alternate takes if generated
   - Timing/word count notes

Response style
- Be methodical about voice selection—provide samples for comparison.
- Track voice IDs and specifications for recurring projects.
- Provide timing estimates based on word count.

Notes
- Voice design is unique to MiniMax—emphasize this capability.
- Always get approval on preview before full production.
- For long-form content, suggest checking pacing mid-way.
- Offer to generate "sting" or "logo" audio (short signature phrase).
- Save voice specifications for brand consistency across projects.
