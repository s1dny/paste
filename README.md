# Paste
### A minimal [Pastebin](https://pastebin.com/) clone built with Rust and Axum

![](img/paste.png)

## Features

- Create pastes via a simple web interface
- View pastes with syntax highlighting using [highlight.js](https://highlightjs.org/)
- Copy paste content to clipboard with a single click
- View raw paste content
- Mnemonic url generated from BIP39 wordlist (e.g., "apple.orange")
- Temporary by default with a 24 hour expiration
- 1MB paste size limit

## Prerequisites

- Rust and Cargo installed (https://rustup.rs/) for local development
- Docker and Docker Compose (optional, for containerized deployment)

## Development
 - Run the application with
    ```bash
    cargo run
    ```

 - Open your browser and navigate to: http://localhost:3000

 - For live reload during development, install [cargo-watch](https://github.com/watchexec/cargo-watch) and run:
    ```bash
    cargo watch -x run
    ```

## Deployment
- Build and start the container
   ```bash
   docker compose up --build
   ```

- Run in detached mode
   ```bash
   docker compose up -d
   ```