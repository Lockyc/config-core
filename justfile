# config-core — task runner

# Recipes run in `sh`, which doesn't inherit cargo from an interactive fish/zsh setup.
export PATH := env_var('HOME') + "/.cargo/bin:" + env_var('PATH')

# `default` pipes `just --list` through a small stock-perl filter that clips long recipe
# docs to your terminal width (…) instead of wrapping. Self-contained — no external files;
# falls back to plain `just --list` where perl is absent. Edit the recipes below, not this.
# List available recipes
default:
    @if command -v perl >/dev/null 2>&1; then just --color always --list | perl -CS -Mutf8 -lpe 'BEGIN{($w)=`stty size 2>/dev/null </dev/tty`=~/ (\d+)/; $w||=100; $col=(-t STDOUT && !exists $ENV{NO_COLOR})} s/\e\[[0-9;]*m//g unless $col; (my $v=$_)=~s/\e\[[0-9;]*m//g; if(length($v)>$w){my($o,$n)=("",0); while(length && $n<$w-1){ if($col && s/^(\e\[[0-9;]*m)//){$o.=$1}else{s/^(.)//;$o.=$1;$n++} } $_=$o."…".($col?"\e[0m":"")}'; else just --list; fi

# Run the test suite (unit + golden formatter tests)
[group("check")]
test:
    cargo test

# Clippy lints, warnings are errors
[group("check")]
lint:
    cargo clippy --all-targets -- -D warnings

# Format all Rust files in place
[group("check")]
fmt:
    cargo fmt

# Build the crate
[group("build")]
build:
    cargo build

# Non-mutating pre-merge gate: rustfmt check, clippy, and the test suite. What CI would run.
[group("check")]
gate:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
