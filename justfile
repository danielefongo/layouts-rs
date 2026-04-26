corpus := "./corpus/english.json,./corpus/italian.json"
config := "config/example.yaml"

analyze *args:
    cargo run --release -- analyze -c {{config}} --corpus {{corpus}} {{args}}

optimize *args:
    cargo run --release -- optimize -c {{config}} --corpus {{corpus}} {{args}}

generate:
    cargo run --release -- generate-optimization-targets -c {{config}} --corpus {{corpus}} --presets ./presets.yaml
