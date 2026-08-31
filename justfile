# caboodle — the whole kit

default:
    @just --list

# Install the stack per a reviewed plan file
install plan="caboodle-plan.toml":
    cargo run -- install --plan "{{plan}}"

# Verify every installed tool: version read-back and controlled functional round-trip
verify plan="caboodle-plan.toml":
    cargo run -- verify --plan "{{plan}}"

check:
    sh -n scripts/install.sh scripts/check-release-assets.sh scripts/test-release-install.sh scripts/setup-environment.sh scripts/build-stack-packs.sh
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features
    scripts/test-release-install.sh

# Build the mdBook (requires mdbook)
book:
    mdbook build docs/book

# Serve the book locally with live reload
book-serve:
    mdbook serve docs/book --open
