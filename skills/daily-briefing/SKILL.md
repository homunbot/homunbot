---
name: daily-briefing
description: Generate a personalized daily briefing with weather, news headlines, and calendar summary. Use this when the user asks for a morning briefing, daily summary, or "what's happening today".
license: MIT
compatibility: Requires internet access for weather and news
allowed-tools: "Web Bash(curl:*)"
metadata:
  author: homunbot
  version: "1.0"
  category: productivity
---

# Daily Briefing

You are generating a personalized daily briefing for the user. Follow this structure:

## Format

Present the briefing in a clean, readable format with these sections:

### 1. Date & Time
- Current date, day of the week
- Any notable dates (holidays, observances)

### 2. Weather
- Use `web_search` to find current weather for the user's location
- Temperature, conditions, forecast for the day
- Any weather alerts or notable changes

### 3. Top News
- Use `web_search` to find top 3-5 news headlines
- Brief 1-line summary for each
- Mix of world, tech, and local news if possible

### 4. Tasks & Reminders
- Check if there are any cron jobs or scheduled reminders for today
- Mention any pending tasks the user previously discussed

## Tone
- Concise and informative
- Friendly but professional
- Use bullet points for readability
- Keep the entire briefing under 500 words

## Example Output

**Daily Briefing - Monday, January 15, 2025**

**Weather:** Rome, 12C, partly cloudy. High 15C, low 8C. No rain expected.

**Top News:**
- Tech: OpenAI announces new model capabilities
- World: EU summit discusses energy policy
- Science: Mars rover discovers new mineral deposits

**Reminders:**
- Team standup at 10:00
- Report deadline: Friday

Have a productive day!
