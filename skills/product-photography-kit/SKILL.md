---
name: product-photography-kit
description: Transform product photos into e-commerce ready images with backgrounds, lifestyle contexts, and marketing variations.
allowed-tools: analyze_image, generate_image, generate_music, tts
---
You are running the Product Photography Kit skill.

Goal
- Take a product photo and generate multiple marketing-ready variations: clean white background, lifestyle scenes, different angles, and marketing copy overlaid.

Ask for
- Product image path.
- Product name, key features, and target audience.
- Use cases (e-commerce listing, social media, print ad, website hero).
- Preferred style (minimalist, lifestyle, premium, playful, technical).
- Any specific scenes or contexts desired for lifestyle variations.
- Whether to generate promotional video version.

Workflow
1) Analyze the original product image:
   - Call analyze_image to understand product characteristics, key features, and best angles.
2) Generate core variations:
   - Clean white background version (e-commerce standard)
   - Lifestyle scene(s) based on product use case
   - Alternative angles or close-up detail shots
3) For each variation:
   - Call generate_image with specific prompt based on analysis
   - Use aspect_ratio appropriate for intended platform (1:1 for e-commerce, 4:5 for Instagram, 16:9 for web)
4) Optional: Generate short product video:
   - Call generate_video with product showcase prompt
   - Use first_frame from hero image
5) Optional: Generate promotional audio:
   - Call tts for product description narration
   - Call generate_music for ambient/background track
6) Return:
   - All image variations with file paths and intended use
   - Video if requested
   - Brief marketing copy suggestions for each image
   - Audio files if generated

Response style
- Organize images by use case and aspect ratio.
- Provide quick marketing copy for each image.
- Note which variations work best for which platform.

Notes
- Analyze first to understand what makes the product marketable.
- White background is essential for e-commerce (Amazon, Shopify, etc.).
- Lifestyle images drive engagement on social media.
- Offer to iterate on specific variations based on feedback.
