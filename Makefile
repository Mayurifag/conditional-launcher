.PHONY: release

release:
ifndef bump
	$(error bump is not set, use 'major', 'minor', or 'patch')
endif
	@echo "Bumping version..."
	@cargo set-version --bump ${bump}
	@{ \
		VERSION=$$(cargo pkgid | cut -d'#' -f2); \
		echo "Creating release for version v$${VERSION}..."; \
		git add Cargo.toml Cargo.lock; \
		git commit -m "Bump version to v$${VERSION}"; \
		git tag "v$${VERSION}"; \
		echo "Pushing to main branch and tags..."; \
		git push origin main; \
		git push origin "v$${VERSION}"; \
		echo "Release v$${VERSION} created and pushed."; \
	}
