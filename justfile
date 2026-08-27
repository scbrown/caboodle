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
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test --all-features
