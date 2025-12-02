.PHONY: setup up down clean logs test health

# Default target
.DEFAULT_GOAL := help

# Colors for output
GREEN := \033[0;32m
YELLOW := \033[0;33m
RED := \033[0;31m
NC := \033[0m # No Color

help: ## Show this help message
	@echo "$(GREEN)Available targets:$(NC)"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  $(YELLOW)%-15s$(NC) %s\n", $$1, $$2}'

setup: ## Copy .env.example to .env if it doesn't exist
	@if [ ! -f .env ]; then \
		cp .env.example .env; \
		echo "$(GREEN)✓ Created .env from .env.example$(NC)"; \
		echo "$(YELLOW)⚠ Please edit .env and set your credentials$(NC)"; \
	else \
		echo "$(YELLOW)⚠ .env already exists, skipping$(NC)"; \
	fi

up: setup ## Start all services with docker-compose
	@echo "$(GREEN)Checking for port conflicts...$(NC)"
	@for port in 4000 3001 4001; do \
		if command -v lsof >/dev/null 2>&1; then \
			if lsof -i :$$port >/dev/null 2>&1; then \
				echo "$(YELLOW)⚠ Port $$port is already in use$(NC)"; \
				lsof -i :$$port | grep -v COMMAND || true; \
			fi; \
		elif command -v netstat >/dev/null 2>&1; then \
			if netstat -tln 2>/dev/null | grep -q ":$$port "; then \
				echo "$(YELLOW)⚠ Port $$port is already in use$(NC)"; \
			fi; \
		fi; \
	done
	@echo "$(GREEN)Starting services...$(NC)"
	docker-compose up -d
	@echo "$(GREEN)Waiting for services to be healthy...$(NC)"
	@timeout 120 bash -c 'until docker-compose ps | grep -q "healthy"; do sleep 2; done' || true
	@echo "$(GREEN)✓ Services are up$(NC)"
	@make health

down: ## Stop all services
	@echo "$(YELLOW)Stopping services...$(NC)"
	docker-compose down

clean: ## Stop services and remove volumes (nuclear option)
	@echo "$(RED)⚠ This will remove all volumes and data$(NC)"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		docker-compose down -v; \
		echo "$(GREEN)✓ Cleaned up$(NC)"; \
	else \
		echo "$(YELLOW)Cancelled$(NC)"; \
	fi

logs: ## Show logs from all services
	docker-compose logs -f

logs-proxy: ## Show logs from vertex-bridge only
	docker-compose logs -f vertex-bridge

logs-harvester: ## Show logs from harvester only
	docker-compose logs -f harvester

logs-bridge: ## Show logs from anthropic-bridge only
	docker-compose logs -f anthropic-bridge

health: ## Check health status of all services
	@echo "$(GREEN)Health Status:$(NC)"
	@docker-compose ps
	@echo ""
	@echo "$(GREEN)Testing endpoints:$(NC)"
	@curl -sf http://localhost:4000/health > /dev/null && echo "$(GREEN)✓ vertex-bridge: OK$(NC)" || echo "$(RED)✗ vertex-bridge: FAILED$(NC)"
	@curl -sf http://localhost:3001/health > /dev/null && echo "$(GREEN)✓ harvester: OK$(NC)" || echo "$(RED)✗ harvester: FAILED$(NC)"
	@curl -sf http://localhost:4001/health > /dev/null && echo "$(GREEN)✓ anthropic-bridge: OK$(NC)" || echo "$(RED)✗ anthropic-bridge: FAILED$(NC)"

test: ## Run Rust tests
	cargo test

build: ## Build all Docker images
	docker-compose build

rebuild: ## Rebuild all Docker images without cache
	docker-compose build --no-cache

ps: ## Show running containers
	docker-compose ps

