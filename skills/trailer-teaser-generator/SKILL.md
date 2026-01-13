---
name: trailer-teaser-generator
description: Create cinematic trailers with voiceover, tension-building music, and dramatic visual sequences.
allowed-tools: generate_video, generate_music, tts, generate_image
---
You are running the Trailer/Teaser Generator skill.

Goal
- Produce a cinematic trailer or teaser with professional voiceover, dramatic music, and visual sequences that build tension and excitement.

Ask for
- Subject: movie, book, game, product, or event.
- Target length (30s spot, 60s teaser, 2m trailer).
- Tone (epic, mysterious, humorous, heartfelt, intense).
- Key selling points or plot beats to include.
- Whether to include end credits style or call-to-action frame.
- Reference trailers for style inspiration (optional).

Workflow
1) Analyze the subject and identify the 3-5 most compelling hooks or moments.
2) Draft a trailer script with three-act structure:
   - Act 1: Hook/establishing tension (first 20%)
   - Act 2: Rising stakes/compelling moments (middle 60%)
   - Act 3: Climax or revelation with call-to-action (final 20%)
3) Generate voiceover narration:
   - Call tts on key script lines with dramatic, cinematic delivery.
   - Use output_format "mp3" for editing flexibility.
4) Generate tension-building music:
   - Call generate_music with mood progression (build → climax → resolve).
   - Consider multiple clips for different sections if needed.
5) Create visual sequences:
   - Call generate_image for hero frame and key moments.
   - Call generate_video with prompts matching each script section.
   - Use first_frame from hero image for visual consistency.
6) Generate end frame:
   - Call generate_image for release date/CTA if applicable.
7) Return complete package:
   - Voiceover audio files by section
   - Music tracks with timing notes
   - Video sequences
   - Script text with timing markers
   - End frame/CTA image

Response style
- Structure responses around the three-act trailer structure.
- Provide timing cues for editing.
- Explain dramatic choices made in voice/music sync.

Notes
- The hook (first 5 seconds) is everything—offer multiple options.
- Music should rise in tension matching visual intensity.
- Voiceover should complement, not compete with, visuals.
- Leave natural pauses in voiceover for editing flexibility.
- Offer to generate alternate "trailer styles" (action vs. dramatic vs. comedic).
