# GeoEngine

Docker-based isolated runtime manager for geospatial workloads with GPU support and ArcGIS Pro/QGIS integration.

## Features

- **Isolated Execution**: Run Python/R scripts in Docker containers with GDAL, PyTorch, and other geospatial libraries
- **GPU Support**: NVIDIA GPU passthrough for CUDA-accelerated processing
- **GIS Integration**: Native plugins for ArcGIS Pro and QGIS
- **Air-gapped Support**: Import/export Docker images for systems without internet access
- **Project Management**: YAML-based project configuration with named scripts
- **Cloud Deployment**: Push images to GCP Artifact Registry

## Quick Start

### Installation

**Linux/macOS/WSL2 (curl):**
```bash
curl -fsSL https://raw.githubusercontent.com/NikaGeospatial/geoengine/main/install/install.sh | bash
```

**macOS (Homebrew):**
```bash
brew tap NikaGeospatial/geoengine
brew install geoengine
```

**Windows (PowerShell as Admin):**
```powershell
irm https://raw.githubusercontent.com/NikaGeospatial/geoengine/main/install/install.ps1 | iex
```

**Offline Installation:**
```bash
# Copy geoengine binary to the target machine, then:
./install.sh --local ./geoengine
```

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) (required)
- [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) (for GPU support)

## Usage

### Create a Project

```bash
# Initialize a new project
geoengine project init --name my-geospatial-project

# Edit geoengine.yaml to configure your project
# Add your Dockerfile, scripts, and data

# Register the project
geoengine project register .

# Build the Docker image
geoengine project build my-geospatial-project

# Run a script
geoengine project run my-geospatial-project train
```

### Run Containers Directly

```bash
# Run a container with GPU and mounts
geoengine run my-image:latest python train.py \
  --gpu \
  --mount ./data:/data \
  --mount ./output:/output \
  --env CUDA_VISIBLE_DEVICES=0 \
  --memory 16g

# Run interactively
geoengine run -t ubuntu:latest bash
```

### Image Management

```bash
# List images
geoengine image list

# Import from tarball (air-gapped)
geoengine image import my-image.tar --tag my-image:latest

# Export for transfer
geoengine image export my-image:latest -o my-image.tar

# Pull from registry
geoengine image pull nvidia/cuda:12.0-base
```

### GIS Integration

```bash
# Start the proxy service
geoengine service start

# Register with ArcGIS Pro
geoengine service register arcgis

# Register with QGIS
geoengine service register qgis

# Check service status
geoengine service status

# View running jobs
geoengine service jobs
```

### Deploy to Cloud

```bash
# Configure GCP authentication
geoengine deploy auth --project my-gcp-project

# Push image to Artifact Registry
geoengine deploy push my-image:latest \
  --project my-gcp-project \
  --region us-central1 \
  --repository geoengine
```

## Project Configuration

Create a `geoengine.yaml` in your project directory:

```yaml
name: land-cover-classifier
version: "1.0"

build:
  dockerfile: ./Dockerfile
  context: .

runtime:
  gpu: true
  memory: "16g"
  cpus: 4
  shm_size: "2g"

  mounts:
    - host: ./data
      container: /data
    - host: ./output
      container: /output

  environment:
    CUDA_VISIBLE_DEVICES: "0"
    PYTHONUNBUFFERED: "1"

  workdir: /workspace

scripts:
  default: python main.py
  train: python train.py --epochs 100
  predict: python predict.py

# GIS tools (optional)
gis:
  tools:
    - name: classify
      label: "Land Cover Classification"
      script: predict
      inputs:
        - name: input_raster
          type: raster
          label: "Input Image"
      outputs:
        - name: output_raster
          type: raster
          label: "Classification"
```

## GIS Plugin Architecture

```
┌─────────────────┐     ┌─────────────────┐
│   ArcGIS Pro    │     │      QGIS       │
│   (Toolbox)     │     │    (Plugin)     │
└────────┬────────┘     └────────┬────────┘
         │                       │
         │   HTTP REST API       │
         └──────────┬────────────┘
                    │
         ┌──────────▼──────────┐
         │   GeoEngine Proxy   │
         │   (localhost:9876)  │
         └──────────┬──────────┘
                    │
    ┌───────────────┼───────────────┐
    │               │               │
┌───▼───┐      ┌───▼───┐       ┌───▼───┐
│Docker │      │Docker │       │Docker │
│  #1   │      │  #2   │       │  #3   │
└───────┘      └───────┘       └───────┘
```

The proxy service:
- Receives job requests from GIS applications
- Manages a queue of processing jobs
- Spawns Docker containers for each job
- Streams results back to the GIS application

## GPU Support

### Linux

Install the NVIDIA Container Toolkit:
```bash
# Ubuntu/Debian
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
  sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
  sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list
sudo apt-get update && sudo apt-get install -y nvidia-container-toolkit
sudo nvidia-ctk runtime configure --runtime=docker
sudo systemctl restart docker
```

### Windows WSL2

1. Install [NVIDIA drivers for WSL](https://developer.nvidia.com/cuda/wsl)
2. Install Docker Desktop with WSL2 backend
3. Enable GPU support in Docker Desktop settings

### macOS

CUDA is not available on macOS. PyTorch will automatically use the MPS (Metal) backend for GPU acceleration.

## API Reference

### REST API Endpoints

When the proxy service is running:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/jobs` | GET | List jobs |
| `/api/jobs` | POST | Submit job |
| `/api/jobs/{id}` | GET | Get job status |
| `/api/jobs/{id}` | DELETE | Cancel job |
| `/api/jobs/{id}/output` | GET | Get job outputs |
| `/api/projects` | GET | List projects |
| `/api/projects/{name}/tools` | GET | Get project tools |

### Job Submission

```json
POST /api/jobs
{
  "project": "my-project",
  "tool": "classify",
  "inputs": {
    "input_raster": "/path/to/image.tif",
    "model": "resnet50"
  },
  "output_dir": "/path/to/outputs"
}
```

## Building from Source

```bash
# Requires Rust 1.70+
git clone https://github.com/NikaGeospatial/geoengine
cd geoengine
cargo build --release

# Binary will be at target/release/geoengine
```

## License

MIT License - see [LICENSE](LICENSE) for details.
