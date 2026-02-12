# Makefile for XST Tool - Tauri Application

.PHONY: help install init-android init-ios init-mobile dev dev-android dev-ios clean build build-android build-ios

# Default target
help:
	@echo "XST Tool - Tauri Development Commands"
	@echo ""
	@echo "Setup Commands:"
	@echo "  install      - Install bun dependencies"
	@echo "  init-android - Initialize Android development"
	@echo "  init-ios     - Initialize iOS development"
	@echo "  init-mobile  - Initialize both Android and iOS"
	@echo ""
	@echo "Development Commands:"
	@echo "  dev          - Run desktop development server"
	@echo "  dev-android  - Run Android development server"
	@echo "  dev-ios      - Run iOS development server"
	@echo ""
	@echo "Build Commands:"
	@echo "  build        - Build desktop application"
	@echo "  build-android - Build Android application"
	@echo "  build-ios    - Build iOS application"
	@echo ""
	@echo "Utility Commands:"
	@echo "  clean        - Clean build artifacts"

# Setup Commands
install:
	bun install

init-android:
	bun run tauri android init

init-ios:
	bun run tauri ios init

init-mobile: init-android init-ios

# Development Commands
dev:
	bun run tauri dev

dev-android:
	bun run tauri android dev

dev-ios:
	bun run tauri ios dev

# Build Commands
build:
	bun run tauri build

build-android:
	bun run tauri android build

build-ios:
	bun run tauri ios build

# Utility Commands
clean:
	bun run clean || true
	rm -rf dist/
	rm -rf src-tauri/target/
	rm -rf node_modules/.cache/

.PHONY: icon
icon:
	npx @tauri-apps/cli icon ./src-tauri/icons/512x512.png

.PHONY: bump
bump:
	@if [ -z "$(VERSION)" ]; then \
		echo "错误: 请提供版本号"; \
		echo "用法: make bump VERSION=0.1.7"; \
		exit 1; \
	fi
	@chmod +x scripts/bump-version.sh
	@./scripts/bump-version.sh $(VERSION)

.PHONY: release
release: bump
	@echo ""
	@echo "开始构建..."
	@bun run tauri build
