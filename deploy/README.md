# TinyIoTHub Deployment Configurations

This directory contains deployment configurations for various environments.

## Directory Structure

```
deploy/
├── docker/          # Docker Compose and Dockerfile configurations
├── kubernetes/      # Kubernetes manifests
│   ├── cloud/       # Cloud service deployment
│   └── edge/        # Edge runtime DaemonSet
└── config/          # Runtime configuration templates
    ├── cloud.toml
    └── edge.toml
```

## Quick Start

### Docker Compose

```bash
cd docker
docker-compose up -d
```

### Kubernetes

```bash
kubectl apply -f kubernetes/cloud/
kubectl apply -f kubernetes/edge/
```
