# escli-rs

[![Build and Test CLI](https://github.com/Anaethelion/escli-rs/actions/workflows/cli-build.yml/badge.svg)](https://github.com/Anaethelion/escli-rs/actions/workflows/cli-build.yml)

A modern, flexible command-line interface (CLI) for interacting with Elasticsearch clusters, written in Rust. This project aims to provide a comprehensive and user-friendly tool for managing and querying Elasticsearch, supporting a wide range of Elasticsearch APIs and features.

## Features
- Full CLI for Elasticsearch APIs
- Auto-completion and shell integration
- Secure authentication (API key, username/password, etc.)
- Support for multiple Elasticsearch versions
- Extensible command structure

---

## Quickstart with .env File

You can configure escli using a `.env` file in your project root or working directory. This allows you to securely manage credentials and connection settings without exposing them on the command line.

**Supported variables:**
- `ESCLI_URL` – Elasticsearch endpoint (e.g., https://localhost:9200)
- `ESCLI_API_KEY` – API key for authentication (recommended)
- `ESCLI_USERNAME` – Username for authentication (alternative)
- `ESCLI_PASSWORD` – Password for authentication (alternative)

**Example `.env` using API key (recommended):**
```env
ESCLI_URL=https://localhost:9200
ESCLI_API_KEY=your_api_key_here
```

**Example `.env` using username and password:**
```env
ESCLI_URL=https://localhost:9200
ESCLI_USERNAME=elastic
ESCLI_PASSWORD=yourpassword
```

> **Tip:** If your .env file is not being picked up, ensure you are running escli from the directory containing the .env file, or that your shell environment is loading it.

---

## Getting Started

Use `--help` to see available commands and options, `-h` for the short version.

### Info
```sh
./escli info
```
![info.gif](tapes/info.gif)

### Bulk
```sh
./escli bulk --input payload.json
```
![bulk.gif](tapes/bulk.gif)

### Search
```sh
./escli search --index my_index <<< '{"query": {"match_all": {}}}'
``` 
![search.gif](tapes/search.gif)

### ES|QL
```sh
./escli esql query --format txt <<< '{"query": "FROM <index> | LIMIT 1"}'
```
![esql.gif](tapes/esql.gif)

### Dump
```sh
./escli utils dump <index>
```
![dump.gif](tapes/dump.gif)

### Prerequisites
- Rust (latest stable or nightly)
- Elasticsearch cluster (local or remote)

### Build
```sh
cargo run -p generator --release
cargo build -p escli --release
```

or

```sh
make release
```

### Usage
```sh
./escli --help
```

### Completions
To enable completions, run and then source the output in your shell:
```sh
COMPLETE=<shell> ./escli
```

---

## Running escli with Docker

You can build and run the escli container locally. By default, the Dockerfile builds for amd64/musl, which works on most modern PCs.

### Build the Docker image (local, single-arch)

```sh
docker build -t escli:latest .
```

### Recommended: Use an alias for escli via Docker

You can set up an alias to run escli as a Docker container, passing environment variables from your host and using the host network:

```sh
alias escli="docker run --network host -i --rm -e ESCLI_URL -e ESCLI_API_KEY ghcr.io/anaethelion/escli:latest"
```

Then, set your environment variables on the host and use escli as if it were installed locally:

```sh
export ESCLI_URL=https://localhost:9200
export ESCLI_API_KEY=your_api_key_here
escli info
```

This approach makes it easy to use escli in Docker with your local environment and network settings.

### Run escli in a container (manual)

You can also run escli directly with Docker:

```sh
docker run --rm -it escli:latest --help
```

Or mount your .env file if needed:

```sh
docker run --rm -it -v $(pwd)/.env:/.env escli:latest info
```

---

## Workspace Structure
- `escli/` - Main CLI application
- `generator/` - Code generation utilities for CLI and API bindings


## Development
- Contributions are welcome! Please open issues or pull requests.
- See each crate's README or source for more details.

## License
This project is licensed under the Apache 2.0 License. See [LICENSE](LICENSE) for details.
