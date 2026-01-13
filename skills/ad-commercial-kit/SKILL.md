---
name: ad-commercial-kit
description: Create professional commercials with hook narration, product demonstration, music, and call-to-action.
allowed-tools: generate_video, generate_image, generate_music, tts
---
You are running the Ad Commercial Kit skill.

Goal
- Produce professional commercial content: hook, product demo, background music, and clear call-to-action for any product or service.

Ask for
- Product/service name, key benefits, and target audience.
- Commercial length (15s, 30s, 60s standard; custom for digital).
- Tone (humorous, heartfelt, urgency-driven, premium, casual).
- Key message or offer to highlight.
- Whether to include:
  - Spoken narration
  - Product demo visuals
  - Testimonial style
  - End card with CTA/offer code
- Budget tier (affordable vs. premium feel).
- Any brand guidelines or must-include elements.

Workflow
1) Develop commercial concept:
   - Identify the single most compelling benefit (the "hook").
   - Structure: Hook (5s) → Problem/Solution (15s) → Benefit Proof (10s) → CTA (5s).
   - Draft script with clear speaker cues and timing markers.
2) Generate hook narration:
   - Call tts on the hook script with energetic, attention-grabbing delivery.
   - Use output_format "mp3" for editing flexibility.
3) Generate product visuals:
   - Call generate_image for product shots and hero frame.
   - Call generate_video with demo-style prompts showing product in use.
   - Use first_frame from hero product image.
4) Generate background music:
   - Call generate_music matching commercial tone (upbeat for fun, cinematic for premium, urgent for limited-time).
   - Ensure music length covers full commercial.
5) Generate end card/CTA:
   - Call generate_image for offer frame with call-to-action text.
   - Include offer code or URL if applicable.
6) Optional: Generate alternate versions:
   - 15s "social" cut for digital ads.
   - 60s "story" cut for longer placements.
   - Audio-only version for podcast/pre-roll.
7) Return commercial package:
   - Script with timing breakdown
   - Narration audio by section
   - Video sequences with first_frame references
   - Music track
   - CTA end card image
   - Alternate versions if generated

Response style
- Focus on the single-minded proposition (one message per commercial).
- Provide timing breakdown for editing reference.
- Suggest A/B testing variations if budget allows.

Notes
- The first 3 seconds determine whether people skip—strong hooks are essential.
- Different platforms have different optimal lengths—ask about placement.
- Music should enhance emotion without overwhelming the message.
- Offer to generate multiple hook options for A/B testing.
- Recommend keeping offer code simple and memorable.
- Note any regulatory considerations (disclaimers, disclosures) if applicable.
