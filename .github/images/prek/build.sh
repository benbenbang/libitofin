#!/bin/bash

docker buildx build \
              --platform linux/amd64,linux/arm64 \
              --push \
              --rm \
              -t ghcr.io/benbenbang/libitofin/prek:latest \
              -f ./.github/images/prek/Dockerfile \
              .
