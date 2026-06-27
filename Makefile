.DEFAULT_GOAL := help

# 使用 bash，并在命令失败、未定义变量或管道失败时立刻退出。
SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c

# 可覆盖的命令与输出目录配置。
CARGO ?= cargo
DIST ?= dist
NODE ?= node
NPM ?= npm
REMOTE ?= origin
SKILL_BIN ?= $(HOME)/.rustcodegraph/bin/rustcodegraph

# 发布流程可通过 make xxx VAR=value 的方式覆盖这些变量。
ARGS ?=
VERSION ?= $(shell $(NODE) -p "require('./package.json').version")
TAG ?= v$(VERSION)
BRANCH ?= $(shell git branch --show-current)
COMMIT_MESSAGE ?= release: $(VERSION)
RELEASE_ADD_PATHS ?= -A
RELEASE_NOTE ?= Published a maintenance release with the latest packaging and release workflow updates.

# 这些目标会串行执行，避免发布步骤互相踩状态。
.NOTPARALLEL: release-prep release-verify release-all publish-tag

.PHONY: help fetch check build release-build release run test fmt fmt-check clippy doc clean install install-skill-bin \
	release-set-version release-prep release-check-version release-dist-plan release-verify \
	release-lint release-ensure-changelog release-check-clean release-commit release-tag \
	release-push release-all publish-tag release-delete-tag

# 从带 ## 的目标注释里自动生成帮助信息。
help: ## Show available commands.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make <target> [ARGS=... VERSION=1.2.3]\n\nTargets:\n"} /^[a-zA-Z0-9_.-]+:.*##/ {printf "  %-22s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# 基础开发命令。
fetch: ## Fetch Rust dependencies.
	$(CARGO) fetch

check: ## Check the Rust code without producing binaries.
	$(CARGO) check

build: ## Build the Rust debug binary.
	$(CARGO) build

release-build: ## Build the Rust release binary.
	$(CARGO) build --release

release: release-build ## Build the Rust release binary.

run: ## Run the Rust CLI. Pass extra args with ARGS="...".
	$(CARGO) run --bin rustcodegraph -- $(ARGS)

test: ## Run Rust tests.
	$(CARGO) test

fmt: ## Format Rust files.
	$(CARGO) fmt

fmt-check: ## Check Rust formatting without changing files.
	$(CARGO) fmt -- --check

clippy: ## Run Rust clippy lints.
	$(CARGO) clippy --all-targets --all-features -- -D warnings

doc: ## Build Rust documentation.
	$(CARGO) doc --no-deps

clean: ## Remove Rust build outputs.
	$(CARGO) clean

install: release ## Install the release binary into ~/.cargo/bin.
	$(CARGO) install --path . --force

install-skill-bin: release ## Build release binary and replace ~/.rustcodegraph/bin/rustcodegraph.
	@mkdir -p "$(dir $(SKILL_BIN))"
	install -m 755 target/release/rustcodegraph "$(SKILL_BIN)"
	@echo "Installed rustcodegraph to $(SKILL_BIN)"

# 将 package.json、Cargo.toml 和 package-lock.json 统一到目标版本。
release-set-version: ## Update Cargo/npm version files. Use VERSION=1.2.3.
	@if [[ ! "$(VERSION)" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$$ ]]; then \
		echo "VERSION must be a SemVer value like 1.2.3; got '$(VERSION)'"; \
		exit 1; \
	fi
	$(NPM) version --no-git-tag-version --allow-same-version "$(VERSION)"
	VERSION="$(VERSION)" perl -0pi -e 's/^version = "[^"]+"/version = "$$ENV{VERSION}"/m' Cargo.toml
	$(NPM) install --package-lock-only --ignore-scripts
	$(CARGO) metadata --format-version=1 >/dev/null

# 预发布准备：更新版本、生成 release changelog，并校验版本一致性。
release-prep: release-set-version ## Update versions and promote CHANGELOG [Unreleased].
	$(CARGO) run --quiet --bin rustcodegraph -- prepare-release "$(VERSION)"
	$(MAKE) release-ensure-changelog VERSION="$(VERSION)" RELEASE_NOTE="$(RELEASE_NOTE)"
	$(MAKE) release-check-version VERSION="$(VERSION)" TAG="$(TAG)"

# 如果本次是空的维护发布，自动补一个最小 changelog 块。
release-ensure-changelog: ## Ensure CHANGELOG has a release block, even for empty maintenance releases.
	@if ! grep -q "^## \[$(VERSION)\]" CHANGELOG.md; then \
		echo "CHANGELOG.md has no ## [$(VERSION)] block; adding a maintenance release note"; \
		awk -v version="$(VERSION)" -v today="$$(date -u +%Y-%m-%d)" -v note="$(RELEASE_NOTE)" '\
			/^## \[Unreleased\]$$/ && !inserted { \
				print; \
				print ""; \
				print "## [" version "] - " today; \
				print ""; \
				print "### Fixes"; \
				print ""; \
				print "- " note; \
				print ""; \
				inserted = 1; \
				skip_blanks = 1; \
				next; \
			} \
			skip_blanks && /^$$/ { next } \
			{ skip_blanks = 0 } \
			{ print } \
			END { if (!inserted) exit 1 } \
		' CHANGELOG.md > CHANGELOG.md.tmp; \
		mv CHANGELOG.md.tmp CHANGELOG.md; \
	fi

# 交叉检查 npm、Cargo、lockfile、tag 与 CHANGELOG 是否完全一致。
release-check-version: ## Verify Cargo, npm, lockfile, CHANGELOG, and TAG agree.
	@pkg="$$( $(NODE) -p "require('./package.json').version" )"; \
	lock="$$( $(NODE) -p "require('./package-lock.json').version" )"; \
	lock_pkg="$$( $(NODE) -p "require('./package-lock.json').packages[''].version" )"; \
	cargo="$$( $(CARGO) metadata --no-deps --format-version=1 | jq -r '.packages[] | select(.name == "rustcodegraph") | .version' )"; \
	expected_tag="v$$pkg"; \
	tag="$(TAG)"; \
	if [[ "$$pkg" != "$$lock" || "$$pkg" != "$$lock_pkg" || "$$pkg" != "$$cargo" ]]; then \
		echo "Version mismatch:"; \
		echo "  package.json:      $$pkg"; \
		echo "  package-lock.json: $$lock"; \
		echo "  lock root package: $$lock_pkg"; \
		echo "  Cargo.toml:        $$cargo"; \
		exit 1; \
	fi; \
	if [[ "$$tag" != "$$expected_tag" ]]; then \
		echo "Release tag $$tag does not match package/Cargo version $$pkg; expected $$expected_tag"; \
		exit 1; \
	fi; \
	if ! grep -q "^## \[$$pkg\]" CHANGELOG.md; then \
		echo "CHANGELOG.md has no ## [$$pkg] release block. Run: make release-prep VERSION=$$pkg"; \
		exit 1; \
	fi; \
	echo "Release inputs are consistent for $$expected_tag"

# 先跑一遍 cargo-dist 计划，尽早发现发布产物配置问题。
release-dist-plan: release-check-version ## Run cargo-dist's release plan for TAG.
	$(DIST) host --steps=create --tag="$(TAG)" --output-format=json > plan-dist-manifest.json
	jq -e '.announcement_tag == "$(TAG)" or .announcement_tag == null' plan-dist-manifest.json >/dev/null
	@echo "cargo-dist plan is valid for $(TAG)"

# 发布前的可选静态检查。
release-lint: fmt-check clippy ## Run optional release lint checks.

# 发布前的核心校验：版本、编译、测试和 dist 计划都必须通过。
release-verify: release-check-version check test release-dist-plan ## Run check, tests, and cargo-dist plan.

# 打 tag / push 前必须保证工作区干净，避免把脏改动带进发布。
release-check-clean: ## Refuse to tag or push from a dirty worktree.
	@if [[ -n "$$(git status --porcelain)" ]]; then \
		echo "Working tree is dirty; commit or stash changes before tagging/pushing:"; \
		git status --short; \
		exit 1; \
	fi

# 如果暂存区里确实有发布改动，就创建一次发布提交；否则跳过。
release-commit: ## Stage and commit release changes if they changed.
	@git add $(RELEASE_ADD_PATHS)
	@if git diff --cached --quiet; then \
		echo "No release changes to commit"; \
	else \
		git commit -m "$(COMMIT_MESSAGE)"; \
	fi

# 仅在 tag 不存在时创建；如果已存在，要求它必须指向当前 HEAD。
release-tag: release-check-version release-check-clean ## Create the release tag locally after verification.
	@if git rev-parse "$(TAG)" >/dev/null 2>&1; then \
		if [[ "$$(git rev-parse "$(TAG)^{}")" != "$$(git rev-parse HEAD)" ]]; then \
			echo "Tag $(TAG) already exists on a different commit"; \
			exit 1; \
		fi; \
		echo "Tag $(TAG) already exists on HEAD"; \
	else \
		git tag -a "$(TAG)" -m "Release $(TAG)"; \
	fi

# 原子推送当前分支和 tag，避免只推成功一半。
release-push: release-check-version release-check-clean ## Push the current branch and release tag together.
	@if [[ -z "$(BRANCH)" ]]; then \
		echo "Cannot infer current branch; pass BRANCH=<branch>"; \
		exit 1; \
	fi
	@if [[ "$$(git rev-parse "$(TAG)^{}")" != "$$(git rev-parse HEAD)" ]]; then \
		echo "Tag $(TAG) does not point at HEAD"; \
		exit 1; \
	fi
	git push --atomic "$(REMOTE)" HEAD:"$(BRANCH)" "$(TAG)"

# 一键完成完整发布链路，并在校验前自动跑一次格式化。
release-all: ## Prep, format, verify, commit, tag, and push a release.
	$(MAKE) release-prep VERSION="$(VERSION)" TAG="$(TAG)"
	$(MAKE) fmt
	$(MAKE) release-verify VERSION="$(VERSION)" TAG="$(TAG)"
	$(MAKE) release-commit VERSION="$(VERSION)" COMMIT_MESSAGE="$(COMMIT_MESSAGE)" RELEASE_ADD_PATHS="$(RELEASE_ADD_PATHS)"
	$(MAKE) release-tag TAG="$(TAG)"
	$(MAKE) release-push TAG="$(TAG)" BRANCH="$(BRANCH)" REMOTE="$(REMOTE)"

# 兼容更直观的目标名。
publish-tag: release-all ## One-command release: verify, test, commit, tag, and push.

# 删除错误的 tag 时要求二次确认，防止误删。
release-delete-tag: ## Delete a bad local+remote tag. Requires OLD_TAG=vX.Y.Z CONFIRM_DELETE_TAG=vX.Y.Z.
	@if [[ -z "$${OLD_TAG:-}" || -z "$${CONFIRM_DELETE_TAG:-}" || "$${OLD_TAG}" != "$${CONFIRM_DELETE_TAG}" ]]; then \
		echo "Refusing to delete a tag without OLD_TAG=<tag> CONFIRM_DELETE_TAG=<same tag>"; \
		exit 1; \
	fi
	git tag -d "$${OLD_TAG}" || true
	git push "$(REMOTE)" ":refs/tags/$${OLD_TAG}"
