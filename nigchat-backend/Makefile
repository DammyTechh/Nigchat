# NigChat backend — common tasks.

.PHONY: help setup up down run check test fmt lint migrate docs clean reset logs

help:
	@echo "make setup    - copy .env, generate secrets, start Postgres and Redis"
	@echo "make run      - run the server locally (cargo run)"
	@echo "make up       - run the full stack in Docker (2 instances + nginx)"
	@echo "make down     - stop the Docker stack"
	@echo "make check    - cargo check + clippy"
	@echo "make test     - run the test suite"
	@echo "make fmt      - format"
	@echo "make docs     - open the API documentation"
	@echo "make reset    - DESTROY local data and re-migrate"

setup:
	@test -f .env || cp .env.example .env
	@echo "Generating secrets into .env..."
	@JWT=$$(openssl rand -hex 32); PEPPER=$$(openssl rand -hex 32); \
	  sed -i.bak "s|^JWT_SECRET=.*|JWT_SECRET=$$JWT|" .env; \
	  sed -i.bak "s|^HASH_PEPPER=.*|HASH_PEPPER=$$PEPPER|" .env; \
	  rm -f .env.bak
	docker compose up -d postgres redis
	@echo "Waiting for PostgreSQL..."
	@until docker compose exec -T postgres pg_isready -U nigchat >/dev/null 2>&1; do sleep 1; done
	@echo "Ready. Now run: make run"

run:
	cargo run -p nigchat-server

up:
	docker compose up --build -d
	@echo "API      http://localhost:8080"
	@echo "Docs     http://localhost:8080/docs"

down:
	docker compose down

logs:
	docker compose logs -f api-1 api-2

check:
	cargo check --workspace --all-targets
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

fmt:
	cargo fmt --all

lint: fmt check

docs:
	@echo "http://localhost:8080/docs"

reset:
	docker compose down -v
	docker compose up -d postgres redis
	@echo "Volumes destroyed. Migrations reapply on next start."

clean:
	cargo clean
