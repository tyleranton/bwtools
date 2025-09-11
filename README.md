# bwtools

A terminal UI tool for StarCraft: Remastered that detects the local web API, identifies your current profile and opponent, and surfaces useful info like ratings and opponent toons. Built with Ratatui and integrates the `bw-web-api-rs` client.

## Features
- Detects your profile and opponent when loading into a game.
- Shows your region and current rating; polls the API every 60s to keep rating fresh.
- Lists opponent toons with gateway/region and highest observed rating per toon.
