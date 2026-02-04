# Publishing Guide

This guide covers publishing the nika-runtime Docker image and the geoengine CLI tool.

## Overview

| Component | Registry | Access |
|-----------|----------|--------|
| `nika-runtime` Docker image | Docker Hub | Public, free |
| `geoengine` CLI binary | GitHub Releases | Public, free |
| Source code | GitHub | Public, free |

---

## Part 1: Publish nika-runtime Docker Image to Docker Hub

Docker Hub provides free public repositories with unlimited pulls.

### 1.1 Create Docker Hub Account

1. Go to https://hub.docker.com/signup
2. Create a free account (e.g., username: `nikaruntime`)
3. Create a repository named `nika-runtime`

### 1.2 Login to Docker Hub

```bash
# Login (will prompt for password)
docker login -u nikaruntime

# Or use access token (recommended for CI)
# Create token at: https://hub.docker.com/settings/security
echo "$DOCKER_TOKEN" | docker login -u nikaruntime --password-stdin
```

### 1.3 Build and Push (Single Architecture)

```bash
cd base-image

# Build
docker build -t nikaruntime/nika-runtime:latest .
docker build -t nikaruntime/nika-runtime:0.1.0 .
docker build -t nikaruntime/nika-runtime:cpu .

# Push
docker push nikaruntime/nika-runtime:latest
docker push nikaruntime/nika-runtime:0.1.0
docker push nikaruntime/nika-runtime:cpu
```

### 1.4 Build and Push (Multi-Architecture - Recommended)

This builds for both AMD64 (Intel/AMD) and ARM64 (Apple Silicon, AWS Graviton):

```bash
# Create buildx builder (one-time setup)
docker buildx create --name multiarch --driver docker-container --use
docker buildx inspect --bootstrap

# Build and push multi-arch image
cd base-image
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t nikaruntime/nika-runtime:latest \
  -t nikaruntime/nika-runtime:0.1.0 \
  -t nikaruntime/nika-runtime:cpu \
  --push .
```

### 1.5 Verify the Image

```bash
# Pull and test
docker pull nikaruntime/nika-runtime:latest
docker run --rm nikaruntime/nika-runtime:latest python -c "import gdal; print(f'GDAL {gdal.__version__}')"
docker run --rm nikaruntime/nika-runtime:latest gdalinfo --version
```

### 1.6 Automated Builds with GitHub Actions

Create `.github/workflows/docker-publish.yml`:

```yaml
name: Publish Docker Image

on:
  push:
    tags:
      - 'v*'
    paths:
      - 'base-image/**'
  workflow_dispatch:

env:
  REGISTRY: docker.io
  IMAGE_NAME: nikaruntime/nika-runtime

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Login to Docker Hub
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.IMAGE_NAME }}
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=raw,value=latest,enable={{is_default_branch}}
            type=raw,value=cpu

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: ./base-image
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

Add secrets to your GitHub repository:
- `DOCKERHUB_USERNAME`: Your Docker Hub username
- `DOCKERHUB_TOKEN`: Docker Hub access token

---

## Part 2: Publish geoengine CLI to GitHub

### 2.1 Create GitHub Repository

```bash
# Initialize git (if not already)
cd /path/to/nika-runtime
git init

# Add remote (replace with your username)
git remote add origin https://github.com/YOUR_USERNAME/geoengine.git

# Initial commit
git add .
git commit -m "Initial commit: geoengine CLI tool"
git push -u origin main
```

### 2.2 Build Release Binaries

Build for multiple platforms:

```bash
# Install cross-compilation targets
rustup target add x86_64-unknown-linux-gnu
rustup target add aarch64-unknown-linux-gnu
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build for Linux x86_64
cargo build --release --target x86_64-unknown-linux-gnu
tar -czvf geoengine-linux-x86_64.tar.gz -C target/x86_64-unknown-linux-gnu/release geoengine

# Build for Linux ARM64 (requires cross or docker)
cargo build --release --target aarch64-unknown-linux-gnu
tar -czvf geoengine-linux-aarch64.tar.gz -C target/aarch64-unknown-linux-gnu/release geoengine

# Build for macOS x86_64 (on macOS)
cargo build --release --target x86_64-apple-darwin
tar -czvf geoengine-darwin-x86_64.tar.gz -C target/x86_64-apple-darwin/release geoengine

# Build for macOS ARM64 (on macOS ARM)
cargo build --release --target aarch64-apple-darwin
tar -czvf geoengine-darwin-aarch64.tar.gz -C target/aarch64-apple-darwin/release geoengine
```

### 2.3 Create a GitHub Release

```bash
# Tag the release
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# Create release via GitHub CLI (install with: brew install gh)
gh release create v0.1.0 \
  --title "GeoEngine v0.1.0" \
  --notes "Initial release of geoengine CLI" \
  geoengine-linux-x86_64.tar.gz \
  geoengine-linux-aarch64.tar.gz \
  geoengine-darwin-x86_64.tar.gz \
  geoengine-darwin-aarch64.tar.gz
```

### 2.4 Automated Releases with GitHub Actions

Create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            archive: tar.gz
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
            archive: tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: tar.gz

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross (Linux ARM64)
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: cargo install cross

      - name: Build
        run: |
          if [ "${{ matrix.target }}" = "aarch64-unknown-linux-gnu" ]; then
            cross build --release --target ${{ matrix.target }}
          else
            cargo build --release --target ${{ matrix.target }}
          fi

      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar -czvf ../../../geoengine-${{ matrix.target }}.${{ matrix.archive }} geoengine
          cd ../../..

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: geoengine-${{ matrix.target }}
          path: geoengine-${{ matrix.target }}.${{ matrix.archive }}

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Download artifacts
        uses: actions/download-artifact@v4

      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            geoengine-*/geoengine-*.tar.gz
          generate_release_notes: true
```

---

## Part 3: Set Up Homebrew Tap (Optional)

For `brew install geoengine`:

### 3.1 Create Homebrew Tap Repository

```bash
# Create a new repo: homebrew-tap
# Structure:
# homebrew-tap/
#   Formula/
#     geoengine.rb
```

### 3.2 Update Formula with SHA256

After releasing, update `install/geoengine.rb` with actual SHA256 hashes:

```bash
# Get SHA256 of release tarballs
shasum -a 256 geoengine-darwin-x86_64.tar.gz
shasum -a 256 geoengine-darwin-aarch64.tar.gz
shasum -a 256 geoengine-linux-x86_64.tar.gz
shasum -a 256 geoengine-linux-aarch64.tar.gz
```

### 3.3 Users Can Install

```bash
brew tap YOUR_USERNAME/tap
brew install geoengine
```

---

## Part 4: Quick Reference

### For Users

```bash
# Install geoengine CLI
curl -fsSL https://raw.githubusercontent.com/YOUR_USERNAME/geoengine/main/install/install.sh | bash

# Pull nika-runtime base image
docker pull nikaruntime/nika-runtime:latest

# Use in Dockerfile
FROM nikaruntime/nika-runtime:latest
```

### Repository URLs (Update These)

After publishing, update these URLs in the codebase:

| File | Update |
|------|--------|
| `Cargo.toml` | `repository = "https://github.com/YOUR_USERNAME/geoengine"` |
| `install/install.sh` | `REPO_URL="https://github.com/YOUR_USERNAME/geoengine"` |
| `install/install.ps1` | `$RepoUrl = "https://github.com/YOUR_USERNAME/geoengine"` |
| `README.md` | All GitHub URLs |
| `base-image/Dockerfile` | Docker Hub username |
| `examples/geoengine.yaml` | `base_image: YOUR_DOCKERHUB/nika-runtime:latest` |

---

## Checklist

- [ ] Create Docker Hub account and repository
- [ ] Build and push nika-runtime image
- [ ] Create GitHub repository
- [ ] Build geoengine CLI binaries
- [ ] Create GitHub release with binaries
- [ ] Set up GitHub Actions for automated builds
- [ ] Update all URLs in codebase
- [ ] Test installation on fresh machine
- [ ] (Optional) Set up Homebrew tap
