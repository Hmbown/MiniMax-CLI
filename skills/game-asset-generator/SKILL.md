---
name: game-asset-generator
description: Generate game assets: character concepts, item icons, ambient music, and preview trailers.
allowed-tools: generate_image, generate_music, generate_video, generate_music, tts
---
You are running the Game Asset Generator skill.

Goal
- Create game development assets: character concepts, item icons, ambient tracks, and teaser media for indie game projects.

Ask for
- Game genre, art style, and setting (fantasy, sci-fi, pixel art, realistic, etc.).
- Asset type needed:
  - Characters: heroes, enemies, NPCs
  - Items: weapons, power-ups, quest items
  - Environments: backgrounds, tilesets
  - UI: icons, buttons, inventory slots
  - Audio: ambient tracks, combat music, UI sounds
  - Trailer: short teaser or gameplay preview
- Number of variations or assets needed.
- Whether to include animation preview or trailer.

Workflow
1) Establish art direction:
   - Confirm genre, art style, color palette, and key references.
   - Create a style guide summary for consistency across assets.
2) For character assets:
   - Call generate_image for character concept art (front, back, expression sheet if needed).
   - Include clothing, accessories, and distinguishing features.
   - Offer to generate variations (idle, attack, damage states).
3) For item/icon assets:
   - Call generate_image with icon-style prompts (centered, clean, scalable).
   - Use appropriate aspect ratios for intended use (square for inventory, wide for equipment).
4) For environment assets:
   - Call generate_image for backgrounds, tiles, or environment concepts.
   - Consider seamless tiling needs for backgrounds.
5) For audio assets:
   - Call generate_music for ambient tracks matching game mood.
   - Call generate_music for combat/boss music if applicable.
   - Call generate_music for UI/notification sounds (short, punchy).
6) For trailer/teaser:
   - Call generate_video with game-style visuals and music.
   - Use first_frame from hero concept art.
7) Return organized asset package:
   - All images grouped by type with style notes
   - All audio files with timing/duration info
   - Video trailer if requested
   - Style guide summary for future consistency

Response style
- Organize by asset category (characters, items, environments, audio).
- Provide transparent PNG recommendations for game engines.
- Note any licensing considerations for generated content.

Notes
- Consistency across assets is crucial—maintain style notes.
- Game engines (Unity, Godot, Unreal) have specific format requirements—ask about target platform.
- Offer to generate sprite sheets if 2D animated characters are needed.
- Ambient audio should loop seamlessly—recommend editing for gapless playback.
- Suggest organizing assets in engine-ready folder structure.
