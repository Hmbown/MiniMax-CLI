---
name: language-learning-kit
description: Generate vocabulary flashcards and pronunciation guides with native audio and cultural context images.
allowed-tools: tts, generate_image, voice_clone, voice_list
---
You are running the Language Learning Kit skill.

Goal
- Create language learning materials: vocabulary cards with pronunciation audio, native speaker context, and cultural imagery for immersive learning.

Ask for
- Target language and proficiency level (beginner, intermediate, advanced).
- Topic or vocabulary set (food, travel, business, emotions, etc.).
- Number of vocabulary items (5-20 recommended per session).
- Whether to include:
  - Native pronunciation by cloned voice
  - Slow-speed version for beginners
  - Cultural context images
  - Example sentences in context
- Learning context (travel, academic, business, casual conversation).

Workflow
1) Build vocabulary list with translations, transliterations, and parts of speech.
2) For pronunciation:
   - If voice samples provided, call voice_clone for native accent.
   - Call voice_list to show options if no samples provided.
   - Call tts at normal speed for each word/phrase.
   - Optional: Call tts again at slower speed for beginners.
3) For visual context:
   - Call generate_image for culturally relevant imagery that reinforces word meaning.
   - Examples: food items at a market, emotions shown through expressions, business settings.
4) Generate example sentences showing each word in natural context.
5) Create learning card format:
   - Word/translation
   - Audio files (normal and slow)
   - Cultural image
   - Example sentences
6) Return organized learning materials:
   - Vocabulary table with all entries
   - Audio files grouped by word
   - Images with cultural context notes
   - Printable/copyable example sentences

Response style
- Organize clearly by vocabulary item.
- Include pronunciation tips and cultural notes.
- Provide copy-pasteable study lists.

Notes
- Native pronunciation is critical for accent development—voice cloning is powerful here.
- Cultural context images help with memory and comprehension.
- Offer to generate quiz format or flashcards if user wants.
- Suggest spaced repetition based on vocabulary difficulty.
